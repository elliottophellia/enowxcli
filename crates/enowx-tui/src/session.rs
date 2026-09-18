#[derive(Clone)]
pub(crate) enum TranscriptKind {
    User,
    Assistant,
    Reasoning,
    Tool {
        id: String,
        name: String,
        args: String,
        result: String,
        running: bool,
        error: bool,
        /// When the call was dispatched, so a long-running tool can show how
        /// long it has been going. `None` for blocks restored from a session
        /// file, where the original wall-clock start is not recoverable and a
        /// duration measured from "now" would be a lie.
        started: Option<std::time::Instant>,
    },
    Notice,
    /// A transient upstream failure being retried. Rendered like an error —
    /// it IS one, the turn just has not given up yet — but as a single line
    /// that updates in place, because a backoff sequence emits one of these
    /// per attempt.
    Retry,
    Error,
    System,
}

#[derive(Clone)]
pub(crate) struct TranscriptBlock {
    pub(crate) kind: TranscriptKind,
    pub(crate) text: String,
}
/// What the running turn is doing right now. The UI animates this so a long
/// model call is visibly alive instead of looking hung.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Activity {
    Idle,
    Waiting,
    Thinking,
    Writing,
    Tool(String),
}

impl Activity {
    pub(crate) fn label(&self) -> &str {
        match self {
            Activity::Idle => "ready",
            Activity::Waiting => "waiting for the model",
            Activity::Thinking => "thinking",
            Activity::Writing => "writing",
            Activity::Tool(name) => name,
        }
    }
}

pub(crate) const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
