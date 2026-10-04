//! The record of every decision: `~/.enx/decisions.jsonl`, one JSON object a
//! line. What was asked, who answered, how sure, how fast, and what the
//! harness did with it, so a use can be judged on real sessions before it is
//! trusted, and later used for evaluation.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::Use;

/// The file grows to this, then starts again beside a copy of itself.
const ROTATE_AT: u64 = 5 * 1024 * 1024;

/// How many records are kept in memory, for the interface.
const KEPT: usize = 200;

/// One decision, or one failure to get one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub at: String,
    /// The use's key: `intent`, `routing`, `ask`, `tool_results`, `shell`.
    #[serde(rename = "use")]
    pub use_key: String,
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    /// `applied`, `shadow` (decided, not acted on), `unsure` (below the
    /// threshold, old path), `timeout`, `error`, or `user` (what the user
    /// did with a decision put to them).
    pub outcome: String,
    /// The answers, in one line.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub answer: String,
    /// The probability the decision turned on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probability: Option<f32>,
    /// What the harness did.
    pub action: String,
}

impl Record {
    pub fn new(u: Use, provider: &str, model: &str, latency_ms: u64, outcome: &str) -> Self {
        Self {
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            use_key: u.key().into(),
            provider: provider.into(),
            model: model.into(),
            latency_ms,
            outcome: outcome.into(),
            answer: String::new(),
            probability: None,
            action: String::new(),
        }
    }

    pub fn answer(mut self, answer: impl Into<String>) -> Self {
        self.answer = answer.into();
        self
    }

    pub fn action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    /// One line for the Log tab: `shell: confirm (risk=destructive 0.97)`.
    pub fn line(&self) -> String {
        let mut line = format!("{}: {}", self.use_key, self.action);
        if self.outcome != "applied" {
            line.push_str(&format!(" [{}]", self.outcome));
        }
        line
    }
}

/// Where decisions are recorded.
#[derive(Clone, Default)]
pub struct DecisionLog {
    path: Option<PathBuf>,
    kept: Arc<Mutex<Vec<Record>>>,
}

/// `~/.enx/decisions.jsonl`.
pub fn path() -> PathBuf {
    crate::config::home_dir().join("decisions.jsonl")
}

impl DecisionLog {
    /// The log file, and memory.
    pub fn file() -> Self {
        Self {
            path: Some(path()),
            kept: Arc::default(),
        }
    }

    /// Memory only: tests.
    pub fn memory() -> Self {
        Self::default()
    }

    /// Record one decision. A log that cannot be written never stops the
    /// work it describes.
    pub fn write(&self, record: &Record) {
        if let Ok(mut kept) = self.kept.lock() {
            kept.push(record.clone());
            let over = kept.len().saturating_sub(KEPT);
            kept.drain(..over);
        }
        let Some(path) = &self.path else {
            return;
        };
        if std::fs::metadata(path).is_ok_and(|m| m.len() > ROTATE_AT) {
            let _ = std::fs::rename(path, path.with_extension("jsonl.1"));
        }
        let Ok(line) = serde_json::to_string(record) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    /// What this process recorded, oldest first.
    pub fn recorded(&self) -> Vec<Record> {
        self.kept.lock().map(|k| k.clone()).unwrap_or_default()
    }
}

/// The last `n` records in the log file, oldest first. Lines that do not
/// parse are skipped.
pub fn read_last(n: usize) -> Vec<Record> {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Vec::new();
    };
    let records: Vec<Record> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let skip = records.len().saturating_sub(n);
    records.into_iter().skip(skip).collect()
}
