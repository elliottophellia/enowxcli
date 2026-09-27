//! Events the loop emits while a turn runs. The HTTP layer forwards these
//! verbatim as SSE, and the CLI renders them to the terminal.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// First event of a turn: which session the turn belongs to.
    Session {
        id: String,
        title: String,
    },
    MessageStart {
        id: String,
    },
    /// Assistant text delta.
    Text {
        delta: String,
    },
    /// Reasoning summary delta, when the provider streams one.
    Reasoning {
        delta: String,
    },
    /// A tool call is about to run.
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    /// A tool call finished.
    ToolResult {
        id: String,
        name: String,
        content: String,
        is_error: bool,
    },
    /// A tool result was carried forward trimmed rather than whole. Reported
    /// so a feature that quietly removes text from the model's context can be
    /// seen working, and judged: silent is indistinguishable from broken.
    Trimmed {
        tool: String,
        /// Characters the result had.
        was: usize,
        /// Characters it was reduced to.
        now: usize,
    },
    /// Incremental output while a tool is still running. UI appends `delta`
    /// to the tool's visible body so long writes and shell output are
    /// visible progressively instead of appearing all at once at the end.
    ToolProgress {
        id: String,
        delta: String,
    },
    /// The just-written file needs a formatter that is not installed. UI
    /// shows a confirmation popup with the install command.
    FormatterMissing {
        language: String,
        bin: String,
        install_hint: String,
        install_cmd: Vec<String>,
    },
    /// Harness note the user should see: step cap, role switch, compaction.
    Notice {
        message: String,
    },
    /// The session changed hands. The transcript shows it, because a reply
    /// that silently changes voice reads as the model behaving oddly rather
    /// than as a different agent answering.
    AgentSwitched {
        to: String,
        reason: String,
    },
    /// The agent asked the user one question or a few and waits for the
    /// answers, given with `Agent::answer` and this `id`.
    Question {
        id: String,
        agent: String,
        questions: Vec<crate::ask::Question>,
    },
    /// A sub-agent started. Carries the branch session id so the interface
    /// can offer to open its transcript while it is still running — a
    /// delegation that only reports when it finishes is indistinguishable
    /// from one that has hung.
    DelegationStarted {
        agent: String,
        task: String,
        session_id: String,
    },
    /// A sub-agent finished; its transcript lives in its own branch session.
    DelegationFinished {
        agent: String,
        summary: String,
        /// The branch holding what it actually did.
        session_id: String,
        /// Whether it ended in failure, so the list can show which.
        failed: bool,
    },
    /// A transient upstream failure that is being retried. Separate from
    /// `Notice` because the UI collapses these into one line — a backoff
    /// sequence otherwise emits a near-identical message per attempt and
    /// buries the conversation — and from `Error` because the turn has not
    /// failed yet.
    Retry {
        message: String,
        attempt: u32,
        max: u32,
    },
    /// Token accounting for the finished model call.
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        context_tokens: u32,
        context_window: u32,
    },
    /// Turn failed. Terminal.
    Error {
        message: String,
    },
    /// Turn finished cleanly. Terminal.
    Done {
        stop_reason: String,
    },
}
