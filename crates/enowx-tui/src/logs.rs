//! What happened this session, as a list rather than as prose.
//!
//! The transcript says what was *said*. It does not say that a turn was
//! handed to another agent, that a request was retried twice, or how long a model call took. When a session stops
//! unexpectedly those are the only things that answer "why", and before this
//! they were either a status line that had already been replaced or nowhere
//! at all.

use std::time::Instant;

/// What a log line is about, which is also how the view filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogKind {
    /// Routing: handoffs and delegations.
    Agent,
    /// One model call: its model, its outcome, what it cost.
    Model,
    /// Something went wrong, or nearly did.
    Problem,
    /// What the decision model decided.
    Decision,
}

impl LogKind {
    pub(crate) fn marker(self) -> &'static str {
        match self {
            LogKind::Agent => "→",
            LogKind::Model => "·",
            LogKind::Problem => "!",
            LogKind::Decision => "◆",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            LogKind::Agent => "agents",
            LogKind::Model => "model",
            LogKind::Problem => "problems",
            LogKind::Decision => "decisions",
        }
    }
}

/// The filters, in the order the key cycles them. `None` shows everything.
pub(crate) const FILTERS: [Option<LogKind>; 5] = [
    None,
    Some(LogKind::Agent),
    Some(LogKind::Model),
    Some(LogKind::Problem),
    Some(LogKind::Decision),
];

#[derive(Debug, Clone)]
pub(crate) struct LogEntry {
    /// When it happened, measured from the start of the session.
    pub(crate) at: Instant,
    pub(crate) kind: LogKind,
    pub(crate) text: String,
    /// The technical line under it: status codes, tokens, timings. Shown when
    /// detail is on, because it answers a different question from the summary
    /// and is noise the rest of the time.
    pub(crate) detail: Option<String>,
}

/// A bounded log. Old lines are dropped rather than growing without limit: a
/// long session would otherwise keep every retry of every call forever.
#[derive(Debug, Default)]
pub(crate) struct Logs {
    entries: std::collections::VecDeque<LogEntry>,
}

impl Logs {
    const MAX: usize = 500;

    pub(crate) fn push(&mut self, kind: LogKind, text: impl Into<String>) {
        self.push_with(kind, text, None::<String>);
    }

    pub(crate) fn push_with(
        &mut self,
        kind: LogKind,
        text: impl Into<String>,
        detail: Option<impl Into<String>>,
    ) {
        if self.entries.len() >= Self::MAX {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry {
            at: Instant::now(),
            kind,
            text: text.into(),
            detail: detail.map(Into::into),
        });
    }

    /// Newest last, matching the filter.
    pub(crate) fn matching(&self, filter: Option<LogKind>) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|entry| filter.is_none_or(|kind| entry.kind == kind))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

/// How far into the session a line happened.
///
/// Time since the session started rather than time of day: what a log is read
/// for is the gap between two lines — a retry that took eight seconds, a
/// model call that took two minutes — and "+4:11" answers that directly where
/// a wall clock makes the reader do the subtraction.
pub(crate) fn since(start: Instant, at: Instant) -> String {
    let secs = at.saturating_duration_since(start).as_secs();
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_keeps_what_was_logged() {
        let mut logs = Logs::default();
        logs.push(LogKind::Agent, "router → fe");
        logs.push(LogKind::Problem, "retry 2/10");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs.matching(None).len(), 2);
        assert_eq!(logs.matching(Some(LogKind::Agent)).len(), 1);
        assert_eq!(logs.matching(Some(LogKind::Agent))[0].text, "router → fe");
    }

    /// A long session must not keep every line forever.
    #[test]
    fn it_is_bounded() {
        let mut logs = Logs::default();
        for i in 0..Logs::MAX + 50 {
            logs.push(LogKind::Model, format!("call {i}"));
        }
        assert_eq!(logs.len(), Logs::MAX);
        // The oldest go, not the newest.
        let newest = logs.matching(None).last().expect("something").text.clone();
        assert_eq!(newest, format!("call {}", Logs::MAX + 49));
    }

    #[test]
    fn the_stamp_reads_as_time_into_the_session() {
        let start = Instant::now();
        assert_eq!(since(start, start), "00:00");
        assert_eq!(
            since(start, start + std::time::Duration::from_secs(95)),
            "01:35"
        );
        assert_eq!(
            since(start, start + std::time::Duration::from_secs(3_725)),
            "1:02:05",
            "past an hour it grows a field rather than wrapping"
        );
    }
}
