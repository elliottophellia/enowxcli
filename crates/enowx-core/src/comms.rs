//! Agents working together: messages between agents at work at the same
//! time, a board they share for one run, and a cross-review in which a
//! reviewer checks a delegate's work and sends it back with corrections.
//!
//! All of it is off unless `agent.comms.enabled` is set (Settings > Team),
//! and each part can be turned off on its own. Like the file contract in
//! `crate::contract`, it lives in memory for the agents of one process.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{json, Value};

/// A message waiting for an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Letter {
    pub from_agent: String,
    pub from_session: String,
    pub text: String,
}

/// A note on the shared board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub agent: String,
    pub topic: String,
    pub text: String,
}

/// A message or a note longer than this is cut: the board and the inboxes
/// are for decisions and questions, not for file contents.
const MAX_TEXT: usize = 2000;
/// Notes kept per run; the oldest go first.
const MAX_NOTES: usize = 200;

/// The inboxes and boards of the agents in this process.
#[derive(Default)]
pub struct Comms {
    inbox: Mutex<HashMap<String, Vec<Letter>>>,
    /// Notes by the run they belong to: the session at its root.
    notes: Mutex<HashMap<String, Vec<Note>>>,
    /// Session id to its parent's, to find a session's root.
    parents: Mutex<HashMap<String, String>>,
}

fn cut(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_TEXT {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(MAX_TEXT).collect();
    out.push_str(" …(cut)");
    out
}

impl Comms {
    /// Record that `child` was delegated from `parent`.
    pub fn branch(&self, child: &str, parent: &str) {
        if let Ok(mut parents) = self.parents.lock() {
            parents.insert(child.to_owned(), parent.to_owned());
        }
    }

    /// The session at the root of `session`'s run.
    pub fn root(&self, session: &str) -> String {
        let parents = self.parents.lock().unwrap_or_else(|e| e.into_inner());
        let mut at = session.to_owned();
        // Bounded: a cycle can only come from a bug, and must not hang.
        for _ in 0..64 {
            match parents.get(&at) {
                Some(parent) => at = parent.clone(),
                None => break,
            }
        }
        at
    }

    /// Leave a message for `to_session`.
    pub fn send(&self, to_session: &str, from_agent: &str, from_session: &str, text: &str) {
        if let Ok(mut inbox) = self.inbox.lock() {
            inbox
                .entry(to_session.to_owned())
                .or_default()
                .push(Letter {
                    from_agent: from_agent.to_owned(),
                    from_session: from_session.to_owned(),
                    text: cut(text),
                });
        }
    }

    /// The messages waiting for `session`, taken out of its inbox.
    pub fn take(&self, session: &str) -> Vec<Letter> {
        self.inbox
            .lock()
            .ok()
            .and_then(|mut inbox| inbox.remove(session))
            .unwrap_or_default()
    }

    /// Put a note on the board of `session`'s run.
    pub fn post(&self, session: &str, agent: &str, topic: &str, text: &str) {
        let root = self.root(session);
        if let Ok(mut notes) = self.notes.lock() {
            let board = notes.entry(root).or_default();
            board.push(Note {
                agent: agent.to_owned(),
                topic: topic.trim().to_owned(),
                text: cut(text),
            });
            if board.len() > MAX_NOTES {
                let extra = board.len() - MAX_NOTES;
                board.drain(..extra);
            }
        }
    }

    /// The notes on the board of `session`'s run, those on `topic` alone
    /// when one is given (matched without regard to case).
    pub fn read(&self, session: &str, topic: Option<&str>) -> Vec<Note> {
        let root = self.root(session);
        let topic = topic
            .map(|t| t.trim().to_lowercase())
            .filter(|t| !t.is_empty());
        self.notes
            .lock()
            .ok()
            .and_then(|notes| notes.get(&root).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter(|note| {
                topic
                    .as_deref()
                    .is_none_or(|t| note.topic.to_lowercase().contains(t))
            })
            .collect()
    }
}

/// Whether a tool call is one of these, handled by the agent loop rather
/// than the tool registry: they need to know who is calling.
pub fn is_comms_tool(name: &str) -> bool {
    matches!(name, "message_agent" | "team_board")
}

/// The tools for what is turned on.
pub fn schemas(messages: bool, board: bool) -> Vec<Value> {
    let mut out = Vec::new();
    if messages {
        out.push(json!({
            "type": "function",
            "function": {
                "name": "message_agent",
                "description":
                    "Send a short message to another agent at work in this run: ask a \
                     question, agree on an interface, warn about a change that affects \
                     its part, or correct something it got wrong. `to` is an agent's id \
                     (`be`, `fe`, `review`, ...) or `lead` for the agent that delegated \
                     to you. It reaches the agent at its next step, and its answer comes \
                     back to you the same way. Do not wait idle for an answer: carry on \
                     with what does not depend on it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "to": {"type": "string", "description": "the agent's id, or `lead`"},
                        "message": {"type": "string", "description": "what to say, plainly and briefly"}
                    },
                    "required": ["to", "message"]
                }
            }
        }));
    }
    if board {
        out.push(json!({
            "type": "function",
            "function": {
                "name": "team_board",
                "description":
                    "The board every agent in this run shares. `post` a decision, an \
                     interface, a convention or a finding the others need (an endpoint's \
                     shape, a token's name, a file that moved); `read` it before starting \
                     work that touches another agent's part, so you build on what was \
                     decided instead of guessing.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {"type": "string", "enum": ["post", "read"]},
                        "topic": {
                            "type": "string",
                            "description": "a short label (`api/orders`, `colors`); for read, only notes on it"
                        },
                        "text": {"type": "string", "description": "for post: the note"}
                    },
                    "required": ["action"]
                }
            }
        }));
    }
    out
}

/// What the system prompt says about working together.
pub fn prompt_note(messages: bool, board: bool, review: bool) -> String {
    if !messages && !board && !review {
        return String::new();
    }
    let mut note = String::from("\n\nWORKING TOGETHER\n");
    if messages {
        note.push_str(
            "- Other agents may be at work at the same time. `message_agent` reaches \
             one; messages to you arrive as `[message from …]`. Answer a question \
             that is put to you, briefly, and keep working. Point out a mistake you see \
             in another agent's part to that agent, with what is wrong and what it \
             should be, rather than working around it.\n",
        );
    }
    if board {
        note.push_str(
            "- `team_board` holds what this run decided. Read it before work that \
             touches another agent's part; post the interfaces and decisions others \
             will build on.\n",
        );
    }
    if review {
        note.push_str(
            "- Work that changes files may be checked by a reviewer, who can send it \
             back with corrections. Fix each one, or say plainly why it is wrong.\n",
        );
    }
    note
}

/// The messages waiting for an agent, as the message it reads them in.
pub fn letters_message(letters: &[Letter]) -> String {
    let mut out = String::new();
    for letter in letters {
        out.push_str(&format!(
            "[message from {} ({})]\n{}\n\n",
            letter.from_agent, letter.from_session, letter.text
        ));
    }
    out.push_str(
        "[harness] Answer with `message_agent` if a reply is wanted, then carry on with \
         your task.",
    );
    out
}

/// The notes on a board, for the agent that read it.
pub fn render_notes(notes: &[Note]) -> String {
    if notes.is_empty() {
        return "The board is empty.".into();
    }
    notes
        .iter()
        .map(|n| {
            if n.topic.is_empty() {
                format!("- {}: {}", n.agent, n.text)
            } else {
                format!("- [{}] {}: {}", n.topic, n.agent, n.text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a reviewer concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    /// With the corrections, as the reviewer wrote them.
    Fix(String),
    /// No verdict line: taken as a pass, with the review shown as written.
    Unclear,
}

/// The reviewer's verdict in its report: a `VERDICT: PASS` or `VERDICT:
/// FIX` anywhere (any case, with or without markdown), the corrections being
/// what follows it.
pub fn parse_verdict(report: &str) -> Verdict {
    let lower = report.to_lowercase();
    let Some(at) = lower.find("verdict") else {
        return Verdict::Unclear;
    };
    let rest = &lower[at + "verdict".len()..];
    let word = rest
        .trim_start_matches(|c: char| c == ':' || c == '*' || c == '`' || c.is_whitespace())
        .split(|c: char| !c.is_ascii_alphabetic())
        .next()
        .unwrap_or("");
    match word {
        "pass" | "passed" | "approved" | "good" => Verdict::Pass,
        "fix" | "fail" | "failed" | "changes" | "needs" => {
            let corrections = report[at..]
                .split_once('\n')
                .map(|(_, after)| after.trim())
                .unwrap_or("")
                .to_owned();
            Verdict::Fix(if corrections.is_empty() {
                report.trim().to_owned()
            } else {
                corrections
            })
        }
        _ => Verdict::Unclear,
    }
}

/// The reviewer's brief: the task, what the delegate reported, the files it
/// changed, and the answer it owes.
pub fn review_brief(agent: &str, task: &str, report: &str, files: &[String]) -> String {
    format!(
        "Review the work `{agent}` just did, then give a verdict.\n\n\
         THE TASK IT WAS GIVEN\n{task}\n\n\
         ITS REPORT\n{report}\n\n\
         FILES IT CHANGED\n{}\n\n\
         Read those files (and what they touch) and check the work against the task: \
         does it do what was asked, is it correct, does it break anything, is it \
         consistent with the code around it. Run what checks you can. Do not change \
         any file yourself.\n\n\
         Start your report's DONE line with `VERDICT: PASS` when the work is right, or \
         `VERDICT: FIX` when it is not, followed by each correction on its own line: \
         the file and line, what is wrong, and what it should be. Only real problems: \
         a matter of taste is not a correction.",
        if files.is_empty() {
            "(none recorded)".to_owned()
        } else {
            files.join("\n")
        }
    )
}

/// What the delegate is told when its work comes back.
pub fn corrections_message(reviewer: &str, corrections: &str, round: u8, rounds: u8) -> String {
    format!(
        "[harness] `{reviewer}` reviewed your work (round {round} of {rounds}) and asks for \
         these corrections:\n\n{corrections}\n\n\
         Fix each one with your tools. If one is wrong, do not apply it: say why in your \
         report. Then end with your report again: DONE, CHANGED, VERIFIED, NEXT."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_waits_until_it_is_taken() {
        let comms = Comms::default();
        comms.send("s-be", "fe", "s-fe", "which field holds the total?");
        let letters = comms.take("s-be");
        assert_eq!(letters.len(), 1);
        assert_eq!(letters[0].from_agent, "fe");
        assert!(comms.take("s-be").is_empty(), "taken once");
        assert!(letters_message(&letters).contains("[message from fe (s-fe)]"));
    }

    #[test]
    fn the_board_is_shared_by_one_run_and_not_another() {
        let comms = Comms::default();
        comms.branch("fe-1", "root-a");
        comms.branch("be-1", "root-a");
        comms.branch("review-1", "be-1");
        comms.branch("fe-2", "root-b");
        comms.post(
            "fe-1",
            "fe",
            "api/orders",
            "GET /orders returns {items, total}",
        );
        assert_eq!(
            comms.read("review-1", None).len(),
            1,
            "a grandchild reads it"
        );
        assert_eq!(comms.read("be-1", Some("API")).len(), 1, "topic, any case");
        assert!(comms.read("be-1", Some("colors")).is_empty());
        assert!(comms.read("fe-2", None).is_empty(), "another run");
    }

    #[test]
    fn a_long_note_is_cut() {
        let comms = Comms::default();
        comms.post("r", "be", "", &"x".repeat(MAX_TEXT * 2));
        let note = &comms.read("r", None)[0];
        assert!(note.text.ends_with("…(cut)"));
    }

    #[test]
    fn verdicts_are_read_however_they_are_dressed() {
        assert_eq!(
            parse_verdict("DONE: VERDICT: PASS, all good"),
            Verdict::Pass
        );
        assert_eq!(parse_verdict("DONE: **Verdict:** pass"), Verdict::Pass);
        match parse_verdict("DONE: VERDICT: FIX\n1. src/a.rs:4 off by one, use <=\nCHANGED: none") {
            Verdict::Fix(c) => assert!(c.starts_with("1. src/a.rs:4"), "{c}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(parse_verdict("DONE: looks fine"), Verdict::Unclear);
    }

    #[test]
    fn the_tools_follow_what_is_on() {
        assert_eq!(schemas(true, true).len(), 2);
        assert_eq!(schemas(false, true).len(), 1);
        assert!(schemas(false, false).is_empty());
        assert!(prompt_note(false, false, false).is_empty());
        assert!(prompt_note(true, false, false).contains("message_agent"));
    }
}
