//! Deciding what a tool result is worth keeping in context.
//!
//! Tool results are 93% of what fills a session's context window, and most of
//! that is three tools — `bash`, `read`, `skill_read` — whose value cannot be
//! read off the name. `bash` is `ls` one moment and a failing test suite the
//! next. So the judgement has to be about the content.
//!
//! What this does NOT do is hide anything from the user. The transcript keeps
//! the full result; this only decides what the model is sent on later turns,
//! which is where the tokens are actually spent.

use serde_json::json;

use crate::systemone::SystemOne;

/// Results below this never go near the network: the judgement would cost
/// more than the text it is judging.
pub const ALWAYS_KEEP_UNDER: usize = 400;

/// How a result should be carried forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// Carry it verbatim.
    Full,
    /// Carry the head and tail with a marker between them. An error message
    /// is usually at one end or the other.
    Trimmed,
}

/// The largest a trimmed result gets: enough for a command, its first output
/// and its failure, without carrying a whole file.
pub const TRIM_TO: usize = 600;

/// Trim a result to its two ends, saying plainly what was dropped so the
/// model does not read the join as the real output.
pub fn trim(content: &str) -> String {
    if content.len() <= TRIM_TO {
        return content.to_owned();
    }
    let half = TRIM_TO / 2;
    let head = floor_boundary(content, half);
    let tail_start = ceil_boundary(content, content.len() - half);
    format!(
        "{}\n… [{} characters dropped from the middle] …\n{}",
        &content[..head],
        content.len() - head - (content.len() - tail_start),
        &content[tail_start..]
    )
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_boundary(s: &str, mut i: usize) -> usize {
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Judge whether a result is worth carrying in full.
///
/// Returns `Keep::Full` whenever there is no judgement to be had — no key, a
/// slow service, a short result. Being wrong in that direction costs tokens;
/// being wrong the other way loses information the model needed.
pub async fn judge(
    system_one: Option<&SystemOne>,
    tool: &str,
    arguments: &str,
    content: &str,
    is_error: bool,
) -> Keep {
    // An error is why the model called the tool at all. Never trim one.
    if is_error || content.len() <= ALWAYS_KEEP_UNDER {
        return Keep::Full;
    }
    let Some(system_one) = system_one else {
        return Keep::Full;
    };
    let answers = system_one
        .ask(
            json!({
                "tool": tool,
                "arguments": truncate_for_prompt(arguments, 400),
                "result": truncate_for_prompt(content, 2000),
            }),
            json!({
                "worth_keeping": {
                    "type": "noul",
                    "instructions":
                        "A coding agent ran this tool and will keep working on the same task. \
                         Does `result` contain specific information it will need to refer back \
                         to later — file contents it must edit, an error it must fix, values it \
                         must use? Answer no when the result merely confirms the tool ran, or \
                         restates what `arguments` already says, or is a listing whose entries \
                         carry nothing beyond their names."
                }
            }),
        )
        .await;
    match answers.as_ref().and_then(|a| a.noul("worth_keeping")) {
        // Deliberately low. The cost of trimming something useful is a wrong
        // answer later; the cost of keeping something useless is some tokens.
        // Only trim when the model is fairly sure there is nothing there.
        Some(noul) if noul.probability < 0.25 => Keep::Trimmed,
        _ => Keep::Full,
    }
}

fn truncate_for_prompt(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    &s[..floor_boundary(s, max)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn without_a_key_everything_is_kept() {
        let long = "x".repeat(10_000);
        assert_eq!(
            judge(None, "bash", "{}", &long, false).await,
            Keep::Full,
            "no judgement available means no change in behaviour"
        );
    }

    /// An error is the reason the turn is still going. Trimming one would cut
    /// exactly the text the model needs.
    #[tokio::test]
    async fn an_error_is_always_kept_in_full() {
        let long = "error: ".to_owned() + &"detail ".repeat(2000);
        assert_eq!(judge(None, "bash", "{}", &long, true).await, Keep::Full);
    }

    #[tokio::test]
    async fn a_short_result_is_never_judged() {
        // No client, but also no need for one: this returns before looking.
        assert_eq!(judge(None, "read", "{}", "ok", false).await, Keep::Full);
    }

    #[test]
    fn trimming_keeps_both_ends() {
        let content = format!(
            "{}{}{}",
            "HEAD".repeat(50),
            "m".repeat(5000),
            "TAIL".repeat(50)
        );
        let trimmed = trim(&content);
        assert!(trimmed.starts_with("HEAD"), "the start survives");
        assert!(trimmed.ends_with("TAIL"), "the end survives");
        assert!(
            trimmed.len() < content.len() / 3,
            "it should actually be shorter: {} vs {}",
            trimmed.len(),
            content.len()
        );
        assert!(
            trimmed.contains("dropped from the middle"),
            "the join must not read as real output: {trimmed}"
        );
    }

    #[test]
    fn short_content_is_returned_unchanged() {
        assert_eq!(trim("hello"), "hello");
    }

    /// Cutting a multi-byte character in half would panic on the slice.
    #[test]
    fn trimming_respects_character_boundaries() {
        let content = "日本語テキスト".repeat(500);
        let trimmed = trim(&content);
        assert!(trimmed.starts_with('日'));
        assert!(trimmed.len() < content.len());
    }
}
