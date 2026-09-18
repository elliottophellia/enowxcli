//! Session compaction: fold older turns into a single summary assistant
//! message so a long conversation stays under the model's context window.
//!
//! Strategy:
//! - Keep the last `keep_last` turns verbatim (default 4).
//! - Ask the model to summarize the rest into a concise state (goal,
//!   decisions, files touched, commands run, key results). One extra API call
//!   per compact.
//! - Replace the old turns with one synthetic assistant turn carrying the
//!   summary. Session file on disk is preserved (append-only history stays).
//!
//! The summarization input keeps tool activity, condensed to one line per
//! call: the tool, its key argument, and a one-line outcome derived from the
//! matching result. File contents and command output never appear — the point
//! is what was touched and what came back, not the bytes.

use std::collections::HashMap;

use anyhow::Result;
use chrono::Utc;
use serde_json::Value;
use tokio::sync::mpsc;

use crate::{
    message::{Message, Role as MessageRole},
    provider::{Chunk, Provider},
    session::{Session, StoredTurn},
};

const SUMMARY_PROMPT: &str = "\
You are compacting a coding-agent conversation to keep it under the model's \
context window. Produce a compact state note the agent can read on the next \
turn to keep working.\n\
\n\
The transcript interleaves prose with condensed tool activity. Lines starting \
with an arrow are one tool call each: the tool name, its key argument, and the \
outcome. They are already summarized — no file contents or command output are \
present, and you must not invent or reconstruct any.\n\
\n\
Include ONLY:\n\
- Goal: what the user is trying to build.\n\
- Decisions: choices already made (libraries, patterns, file paths).\n\
- Files: which files were created, edited, or read, with a one-line purpose.\n\
- Commands: commands that were run and what they returned (pass, fail, exit).\n\
- Open: what is not done yet or is blocked.\n\
\n\
Do NOT restate every tool call, do NOT echo file contents or command output \
back, do NOT include code blocks, and do NOT apologize for summarizing. Write \
in tight, factual bullets.\n";

/// Roughly how wide one condensed tool line may get before it is trimmed.
/// Keeps a session's tool activity in the hundreds of tokens, not thousands.
const TOOL_LINE_BUDGET: usize = 80;
/// Longest key argument kept on a tool line; longer ones are elided in the
/// middle so both the start and the tail (often the distinguishing part of a
/// path) survive.
const ARG_BUDGET: usize = 48;
/// Longest outcome fragment kept on a tool line.
const OUTCOME_BUDGET: usize = 40;

/// One condensed tool line: `read src/auth.rs (240 lines)`.
fn tool_line(call: &crate::message::ToolCall, result: Option<&Message>) -> String {
    let args: Value = serde_json::from_str(&call.arguments).unwrap_or(Value::Null);
    let argument = key_argument(&call.name, &args);
    let outcome = result.map(|message| outcome_of(&call.name, message));

    let mut line = String::from("  \u{2192} ");
    line.push_str(&call.name);
    if let Some(argument) = argument {
        line.push(' ');
        line.push_str(&elide(argument.trim(), ARG_BUDGET));
    }
    match outcome.as_deref() {
        Some(outcome) if !outcome.is_empty() => {
            line.push_str(" \u{2192} ");
            line.push_str(&elide(outcome, OUTCOME_BUDGET));
        }
        _ => {}
    }
    // Belt and braces: even a pathological name plus argument stays bounded.
    elide(&line, TOOL_LINE_BUDGET + 4)
}

/// The argument worth naming for a tool call. Derived from the tool schemas in
/// `tools/`: path-ish tools take `path`, search tools `pattern`, shell
/// `command`. Unknown tools (MCP proxies) fall back to the first string field.
fn key_argument(name: &str, args: &Value) -> Option<String> {
    let string = |key: &str| args.get(key).and_then(Value::as_str).map(str::to_string);
    match name {
        "read" | "write" | "edit" => string("path"),
        "glob" | "grep" => string("pattern"),
        "bash" => string("command").map(|command| command.replace('\n', " ")),
        "fetch" => string("url"),
        "skill_read" => string("name"),
        "todo" => None,
        _ => args
            .as_object()
            .and_then(|map| {
                map.values()
                    .find_map(|value| value.as_str().map(str::to_string))
            })
            .map(|value| value.replace('\n', " ")),
    }
}

/// A cheap, true one-liner about what a tool call returned. Every branch reads
/// counts or the tool's own status line; none of them carries payload text.
fn outcome_of(name: &str, result: &Message) -> String {
    let content = result.content.trim();
    if result.error.is_some() {
        return first_line(content)
            .map_or_else(|| "failed".to_string(), |line| format!("failed: {line}"));
    }
    if content.is_empty() {
        return String::new();
    }
    match name {
        "read" | "fetch" | "skill_read" => format!("{} lines", content.lines().count()),
        "glob" | "grep" => {
            if content == "No matches." {
                "no matches".to_string()
            } else {
                format!("{} matches", content.lines().count())
            }
        }
        // `write` and `edit` already answer in one status line
        // ("Wrote 412 bytes to src/auth.rs", "Updated src/auth.rs at line 88").
        "write" | "edit" => first_line(content).unwrap_or_default(),
        // Shell results start with `exit N`; keep the code and a line count,
        // never the output itself.
        "bash" => {
            let exit = first_line(content).unwrap_or_default();
            let rest = content.lines().skip(1).filter(|l| !l.is_empty()).count();
            if rest == 0 {
                exit
            } else {
                format!("{exit}, {rest} lines")
            }
        }
        _ => format!("{} lines", content.lines().count()),
    }
}

fn first_line(content: &str) -> Option<String> {
    content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// Shorten `text` to `budget` characters, keeping the head and the tail. Counts
/// characters, not bytes, so it never splits a multi-byte char.
fn elide(text: &str, budget: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= budget {
        return text.to_string();
    }
    if budget <= 3 {
        return chars[..budget].iter().collect();
    }
    let tail = (budget - 3) / 3;
    let head = budget - 3 - tail;
    let mut out: String = chars[..head].iter().collect();
    out.push_str("...");
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// Build the text handed to the summarizer from the turns being folded.
///
/// Pure and provider-free so it can be tested directly. Shape:
///
/// ```text
/// USER: fix auth
/// ASSISTANT: let me look
///   → read src/auth.rs → 240 lines
///   → bash cargo test → exit 0, 12 lines
/// ```
///
/// Tool results are consumed only to derive outcomes; they never appear as
/// their own lines, so file contents stay out of the summarization input.
pub fn build_transcript(turns: &[StoredTurn]) -> String {
    // Tool results arrive in later turns than the call that asked for them, so
    // index them by id up front rather than rescanning per call.
    let results: HashMap<&str, &Message> = turns
        .iter()
        .filter(|turn| turn.message.role == MessageRole::Tool)
        .filter_map(|turn| {
            turn.message
                .tool_call_id
                .as_deref()
                .map(|id| (id, &turn.message))
        })
        .collect();

    let mut out = String::new();
    for turn in turns {
        let message = &turn.message;
        match message.role {
            MessageRole::User => {
                let content = message.content.trim();
                if !content.is_empty() {
                    out.push_str(&format!("USER: {content}\n"));
                }
            }
            MessageRole::Assistant => {
                let content = message.content.trim();
                if !content.is_empty() {
                    out.push_str(&format!("ASSISTANT: {content}\n"));
                }
                // An assistant turn whose content is empty because it carries
                // tool_calls still contributes: those lines are the work.
                for call in &message.tool_calls {
                    out.push_str(&tool_line(call, results.get(call.id.as_str()).copied()));
                    out.push('\n');
                }
            }
            // Tool results are folded into their call's line above.
            MessageRole::Tool | MessageRole::System => {}
        }
    }
    out
}

/// Perform a compact on `session` using `provider`. Returns the summary text
/// so callers can log or display it. Does nothing when the session has fewer
/// than `keep_last + 2` turns (nothing meaningful to fold).
/// A turn scoring at or above this is carried verbatim past the fold.
/// Levels are 0..=3 and 2 means "still being worked on", so this rescues
/// live work without rescuing everything.
const STILL_LIVE_AT: f32 = 1.8;

/// The most turns rescued from the fold. Rescuing everything would leave
/// nothing to compact, which is the one outcome worse than compacting badly.
const MAX_RESCUED: usize = 6;

/// Score the turns about to be folded and return the indices worth keeping
/// verbatim. Empty whenever there is no judgement to be had, which leaves
/// compaction exactly as it was.
async fn rescue_live_turns(
    system_one: Option<&crate::systemone::SystemOne>,
    goal: &str,
    older: &[StoredTurn],
) -> Vec<usize> {
    let Some(system_one) = system_one else {
        return Vec::new();
    };
    // Only the tail of the fold is worth asking about: a turn twenty back is
    // being summarised for a reason.
    let window = 10.min(older.len());
    let start = older.len() - window;
    let mut questions = serde_json::Map::new();
    let mut state = serde_json::Map::new();
    state.insert("goal".into(), goal.into());
    for (offset, turn) in older[start..].iter().enumerate() {
        let body = turn.message.content.chars().take(700).collect::<String>();
        state.insert(format!("turn_{offset}"), body.into());
        questions.insert(
            format!("live_{offset}"),
            serde_json::json!({
                "type": "score",
                "instructions": format!(
                    "`goal` is what the user is working on. How live is `turn_{offset}` \
                     for continuing that work right now?"
                ),
                "criteria": [
                    "Finished and superseded; nothing in it will be referred to again",
                    "Background only; the outcome matters but the detail does not",
                    "Still being worked on; its specifics are needed to continue",
                    "The current task; work would stall without it"
                ]
            }),
        );
    }
    if questions.is_empty() {
        return Vec::new();
    }
    let Some(answers) = system_one
        .ask(
            serde_json::Value::Object(state),
            serde_json::Value::Object(questions),
        )
        .await
    else {
        return Vec::new();
    };
    let mut scored: Vec<(usize, f32)> = (0..window)
        .filter_map(|offset| {
            let score = answers.score(&format!("live_{offset}"))?;
            (score.value >= STILL_LIVE_AT).then_some((start + offset, score.value))
        })
        .collect();
    // Highest first, so the cap keeps the liveliest rather than the earliest.
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(MAX_RESCUED);
    let mut indices: Vec<usize> = scored.into_iter().map(|(i, _)| i).collect();
    // Back into transcript order: the model reads them as a conversation.
    indices.sort_unstable();
    indices
}

pub async fn compact(
    session: &mut Session,
    provider: &Provider,
    keep_last: usize,
) -> Result<Option<String>> {
    compact_with(session, provider, keep_last, None).await
}

/// Compaction with an optional judge for which older turns are still live.
///
/// Without one this is the positional fold it has always been: the last
/// `keep_last` turns verbatim, everything before them summarised. The cut is
/// by age alone, so a turn from twenty minutes ago that the work still rests
/// on is folded away along with everything genuinely finished.
pub async fn compact_with(
    session: &mut Session,
    provider: &Provider,
    keep_last: usize,
    system_one: Option<&crate::systemone::SystemOne>,
) -> Result<Option<String>> {
    if session.turns.len() < keep_last + 2 {
        return Ok(None);
    }
    let split = session.turns.len() - keep_last;
    let (mut older, keep): (Vec<StoredTurn>, Vec<StoredTurn>) = {
        let mut turns = std::mem::take(&mut session.turns);
        let keep = turns.split_off(split);
        (turns, keep)
    };

    // The most recent user message says what the work is; scoring against it
    // is what makes "still live" mean anything.
    let goal = keep
        .iter()
        .chain(older.iter())
        .rev()
        .find(|t| t.message.role == MessageRole::User)
        .map(|t| t.message.content.chars().take(500).collect::<String>())
        .unwrap_or_default();
    let rescued_indices = rescue_live_turns(system_one, &goal, &older).await;
    let rescued: Vec<StoredTurn> = rescued_indices
        .iter()
        .filter_map(|i| older.get(*i).cloned())
        .collect();
    // A rescued turn is carried verbatim, so it must not also be summarised —
    // the model would read the same work twice and treat it as two attempts.
    for index in rescued_indices.iter().rev() {
        older.remove(*index);
    }

    // Prose plus condensed tool activity, built from the folded turns.
    let transcript = build_transcript(&older);

    if transcript.trim().is_empty() {
        // Nothing to summarize. Restore and bail out cleanly.
        session.turns = older;
        session.turns.extend(rescued);
        session.turns.extend(keep);
        return Ok(None);
    }

    let messages = vec![Message::system(SUMMARY_PROMPT), Message::user(transcript)];
    let (tx, mut rx) = mpsc::channel::<Chunk>(64);
    // Drain the sink so the streaming client does not block on backpressure;
    // we do not surface partial chunks to the UI during compact.
    tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let completion = provider.complete(&messages, &[] as &[Value], &tx).await?;
    let summary = completion.text.trim().to_string();
    if summary.is_empty() {
        session.turns = older;
        session.turns.extend(rescued);
        session.turns.extend(keep);
        return Ok(None);
    }

    // Replace older turns with one synthetic assistant turn carrying the
    // summary. Keep the trailing `keep_last` turns as-is so the agent still
    // sees recent context verbatim.
    let now = Utc::now();
    let summary_turn = StoredTurn {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: now,
        message: Message {
            role: MessageRole::Assistant,
            content: format!("[COMPACTED SUMMARY]\n{summary}"),
            reasoning: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        },
    };
    session.turns = vec![summary_turn];
    session.turns.extend(rescued);
    session.turns.extend(keep);
    session.updated_at = now;
    Ok(Some(summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::ToolCall;

    fn turn(message: Message) -> StoredTurn {
        StoredTurn {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            message,
        }
    }

    fn calls(pairs: &[(&str, &str, &str)]) -> Message {
        let mut message = Message::assistant("");
        message.tool_calls = pairs
            .iter()
            .map(|(id, name, arguments)| ToolCall {
                id: (*id).to_string(),
                name: (*name).to_string(),
                arguments: (*arguments).to_string(),
            })
            .collect();
        message
    }

    #[test]
    fn tool_calls_name_their_tool_and_argument() {
        let turns = vec![
            turn(Message::user("fix auth")),
            turn(calls(&[
                ("c1", "read", r#"{"path":"src/auth.rs"}"#),
                ("c2", "bash", r#"{"command":"cargo test"}"#),
                ("c3", "grep", r#"{"pattern":"token"}"#),
            ])),
            turn(Message::tool_result("c1", "1:use x;\n2:fn main() {}")),
            turn(Message::tool_result(
                "c2",
                "exit 0\ntest result: ok. 41 passed",
            )),
            turn(Message::tool_result("c3", "src/auth.rs:12:token")),
            turn(Message::assistant("done")),
        ];
        let transcript = build_transcript(&turns);

        assert!(transcript.contains("USER: fix auth"), "{transcript}");
        assert!(transcript.contains("read src/auth.rs"), "{transcript}");
        assert!(transcript.contains("bash cargo test"), "{transcript}");
        assert!(transcript.contains("grep token"), "{transcript}");
        assert!(transcript.contains("ASSISTANT: done"), "{transcript}");
        // Outcomes are counts and status, not payload.
        assert!(transcript.contains("2 lines"), "{transcript}");
        assert!(transcript.contains("exit 0"), "{transcript}");
        assert!(transcript.contains("1 matches"), "{transcript}");
    }

    #[test]
    fn file_contents_never_reach_the_transcript() {
        let secret = "SUPER_SECRET_FILE_BODY";
        let huge: String = (0..5_000)
            .map(|index| format!("{index}:{secret} line filler\n"))
            .collect();
        let turns = vec![
            turn(Message::user("read it")),
            turn(calls(&[("c1", "read", r#"{"path":"src/big.rs"}"#)])),
            turn(Message::tool_result("c1", huge.clone())),
        ];
        let transcript = build_transcript(&turns);

        assert!(!transcript.contains(secret), "file body leaked");
        assert!(transcript.contains("read src/big.rs"), "{transcript}");
        assert!(transcript.contains("5000 lines"), "{transcript}");
        assert!(
            transcript.len() < huge.len() / 100,
            "transcript {} not far smaller than result {}",
            transcript.len(),
            huge.len()
        );
    }

    #[test]
    fn command_output_never_reaches_the_transcript() {
        let noisy = format!(
            "exit 1\n{}",
            "compiler spew with SECRET_TOKEN\n".repeat(500)
        );
        let turns = vec![
            turn(calls(&[("c1", "bash", r#"{"command":"cargo build"}"#)])),
            turn(Message::tool_result("c1", noisy)),
        ];
        let transcript = build_transcript(&turns);

        assert!(!transcript.contains("SECRET_TOKEN"), "{transcript}");
        assert!(transcript.contains("exit 1"), "{transcript}");
    }

    /// The regression this module exists for: an assistant turn with empty
    /// content because it carries `tool_calls` used to be dropped whole, which
    /// discarded most of a working session.
    #[test]
    fn assistant_turn_with_only_tool_calls_still_contributes() {
        let only_calls = calls(&[
            (
                "c1",
                "edit",
                r#"{"path":"src/auth.rs","old_text":"a","new_text":"b"}"#,
            ),
            (
                "c2",
                "write",
                r#"{"path":"src/new.rs","content":"fn main() {}"}"#,
            ),
        ]);
        assert!(
            only_calls.content.trim().is_empty(),
            "fixture must have empty content to exercise the bug"
        );
        let turns = vec![
            turn(Message::user("rename it")),
            turn(only_calls),
            turn(Message::tool_result("c1", "Updated src/auth.rs at line 88")),
            turn(Message::tool_result("c2", "Wrote 12 bytes to src/new.rs")),
        ];
        let transcript = build_transcript(&turns);

        assert!(
            transcript.contains("edit src/auth.rs"),
            "REGRESSION: tool-call-only assistant turn was dropped: {transcript}"
        );
        assert!(
            transcript.contains("write src/new.rs"),
            "REGRESSION: tool-call-only assistant turn was dropped: {transcript}"
        );
        assert!(
            transcript.contains("Updated src/auth.rs at line 88"),
            "{transcript}"
        );
        // The `content` argument of `write` must not be echoed back.
        assert!(!transcript.contains("fn main() {}"), "{transcript}");
    }

    #[test]
    fn a_huge_argument_stays_within_the_line_budget() {
        let long_path = format!("src/{}/mod.rs", "deeply/nested".repeat(60));
        let long_command = "cargo test --workspace -- --nocapture ".repeat(40);
        let turns = vec![
            turn(calls(&[
                ("c1", "read", &format!(r#"{{"path":"{long_path}"}}"#)),
                ("c2", "bash", &format!(r#"{{"command":"{long_command}"}}"#)),
            ])),
            turn(Message::tool_result("c1", "1:x")),
            turn(Message::tool_result("c2", "exit 0\nfine")),
        ];
        let transcript = build_transcript(&turns);

        for line in transcript.lines().filter(|line| line.contains('\u{2192}')) {
            assert!(
                line.chars().count() <= TOOL_LINE_BUDGET + 4,
                "tool line {} chars, over budget: {line}",
                line.chars().count()
            );
        }
        assert!(transcript.contains("read src/"), "{transcript}");
        assert!(transcript.contains("bash cargo test"), "{transcript}");
    }

    #[test]
    fn pure_prose_sessions_still_work() {
        let turns = vec![
            turn(Message::user("what is a trait object")),
            turn(Message::assistant("a fat pointer with a vtable")),
            turn(Message::user("thanks")),
        ];
        let transcript = build_transcript(&turns);

        assert_eq!(
            transcript,
            "USER: what is a trait object\nASSISTANT: a fat pointer with a vtable\nUSER: thanks\n"
        );
        assert!(!transcript.contains('\u{2192}'));
    }

    #[test]
    fn a_failed_tool_call_is_reported_as_failed() {
        let mut failure = Message::tool_result("c1", "exit 1\nerror: no such file");
        failure.error = Some("Tool execution failed".into());
        let turns = vec![
            turn(calls(&[("c1", "read", r#"{"path":"gone.rs"}"#)])),
            turn(failure),
        ];
        let transcript = build_transcript(&turns);

        assert!(transcript.contains("read gone.rs"), "{transcript}");
        assert!(transcript.contains("failed"), "{transcript}");
    }

    #[test]
    fn a_call_without_a_result_still_gets_a_line() {
        let turns = vec![
            turn(Message::user("go")),
            turn(calls(&[("c1", "glob", r#"{"pattern":"src/**/*.rs"}"#)])),
        ];
        let transcript = build_transcript(&turns);

        assert!(transcript.contains("glob src/**/*.rs"), "{transcript}");
    }

    #[test]
    fn unknown_tools_fall_back_to_their_first_string_argument() {
        let turns = vec![
            turn(calls(&[("c1", "mcp__x__query", r#"{"q":"select 1"}"#)])),
            turn(Message::tool_result("c1", "one\ntwo")),
        ];
        let transcript = build_transcript(&turns);

        assert!(
            transcript.contains("mcp__x__query select 1"),
            "{transcript}"
        );
        assert!(transcript.contains("2 lines"), "{transcript}");
    }
}

#[cfg(test)]
mod rescue_tests {
    use super::*;

    fn turn(message: Message) -> StoredTurn {
        StoredTurn {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            message,
        }
    }

    fn older(n: usize) -> Vec<StoredTurn> {
        (0..n)
            .map(|i| turn(Message::user(format!("turn {i}"))))
            .collect()
    }

    /// The whole feature is opt-in. With no judge, nothing is rescued and
    /// compaction is the positional fold it has always been.
    #[tokio::test]
    async fn no_judge_rescues_nothing() {
        let turns = older(10);
        assert!(
            rescue_live_turns(None, "build the parser", &turns)
                .await
                .is_empty(),
            "an unconfigured install must behave exactly as before"
        );
    }

    /// A judge that cannot be reached is the same as no judge: the network is
    /// not allowed to change what compaction does.
    #[tokio::test]
    async fn an_unreachable_judge_rescues_nothing() {
        let config = crate::config::TypeSafeConfig {
            api_key: "not-a-real-key".into(),
            // Unroutable, so this fails fast rather than waiting.
            base_url: "http://127.0.0.1:1/v1/systemone".into(),
            timeout_ms: 300,
            ..crate::config::TypeSafeConfig::default()
        };
        let judge = crate::systemone::SystemOne::new(&config).expect("a key means a client");
        let turns = older(10);
        assert!(
            rescue_live_turns(Some(&judge), "build the parser", &turns)
                .await
                .is_empty(),
            "a failed judgement must not change the fold"
        );
    }
}
