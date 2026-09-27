#[derive(Clone)]
pub(crate) enum TranscriptKind {
    User,
    Assistant,
    /// The model's thinking, drawn as one row (`Thought for 6s`) with its
    /// text a click or Ctrl+R away.
    Reasoning {
        /// Key for opening it, as a tool call's id is.
        id: String,
        /// When it began streaming. `None` when replayed from a session file.
        started: Option<std::time::Instant>,
        /// How long it took, set once something else began. `None` while it
        /// streams, and when replayed.
        elapsed: Option<std::time::Duration>,
    },
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
    /// One delegation, start to finish, as one row: who was sent, whether it
    /// is still going, and the report once it is back. The text is the task.
    /// That stays closed by default: it is written for the sub-agent, and a
    /// full brief is forty lines of instructions that push the conversation
    /// off screen. The report is written for the reader and is always shown.
    Brief {
        agent: String,
        /// The branch session, so the row can be clicked open the same way a
        /// tool result is.
        id: String,
        state: crate::app::DelegationState,
        /// Empty until the sub-agent has finished.
        report: String,
    },
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
