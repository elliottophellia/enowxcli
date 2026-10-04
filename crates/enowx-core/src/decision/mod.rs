//! Decision models: small typed judgements made by a model built for them.
//!
//! A decision model does not write. It reads a state and answers typed
//! questions (yes or no, one of a set, a level on a scale) with a probability
//! for each answer, fast. The harness uses that where it now guesses from a
//! long prompt or a rule: whether a request asks for a brainstorm, which
//! specialist a task is for, whether a question is the user's to answer,
//! whether a tool result is worth carrying, whether a shell command needs the
//! user's word first.
//!
//! The provider is swappable. [`DecisionProvider`] is the whole interface;
//! Clef on Cloudflare Workers AI and TypeSafe's Jev are built in, any
//! endpoint speaking the same API can be added, and a chat model already
//! configured in enowx can stand in by answering in JSON.
//!
//! Every judgement is optional by construction. Off by default; when on, a
//! late or failed answer falls back to what runs without one, and every
//! decision is written to `~/.enx/decisions.jsonl`.

mod jev;
mod llm;
pub mod log;
pub mod uses;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub use jev::JevCompat;
pub use llm::LlmJudge;
pub use log::{DecisionLog, Record};

/// What kind of answer a question takes.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Yes or no, answered with the probability of yes.
    Bool,
    /// One of these keys, each with what it means.
    Choice(Vec<(String, String)>),
    /// A level on this ordered scale, lowest first.
    Score(Vec<String>),
}

/// One typed question about the state.
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub id: String,
    pub instructions: String,
    pub kind: Kind,
}

impl Question {
    pub fn boolean(id: &str, instructions: &str) -> Self {
        Self {
            id: id.into(),
            instructions: instructions.into(),
            kind: Kind::Bool,
        }
    }

    pub fn choice(id: &str, instructions: &str, options: &[(&str, &str)]) -> Self {
        Self::choice_owned(
            id,
            instructions,
            options
                .iter()
                .map(|(k, d)| ((*k).to_owned(), (*d).to_owned()))
                .collect(),
        )
    }

    pub fn choice_owned(id: &str, instructions: &str, options: Vec<(String, String)>) -> Self {
        Self {
            id: id.into(),
            instructions: instructions.into(),
            kind: Kind::Choice(options),
        }
    }

    pub fn score(id: &str, instructions: &str, levels: &[&str]) -> Self {
        Self {
            id: id.into(),
            instructions: instructions.into(),
            kind: Kind::Score(levels.iter().map(|l| (*l).to_owned()).collect()),
        }
    }
}

/// One answer.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// The probability the answer is yes.
    Bool { yes: f32 },
    /// The chosen key, and the probability of each key the provider gave.
    Choice {
        key: String,
        probabilities: BTreeMap<String, f32>,
    },
    /// A position on the scale, counted from 0, which may fall between two
    /// levels, and how concentrated the answer was.
    Score { value: f32, confidence: f32 },
}

/// A provider's answers to one call.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Decision {
    pub answers: BTreeMap<String, Answer>,
    pub latency_ms: u64,
    pub provider: String,
    pub model: String,
    /// The old path was used instead: no answer in time, or none at all.
    pub fallback_used: bool,
}

impl Decision {
    /// The probability of yes.
    pub fn yes(&self, id: &str) -> Option<f32> {
        match self.answers.get(id)? {
            Answer::Bool { yes } => Some(*yes),
            _ => None,
        }
    }

    /// The chosen key and its probability.
    pub fn choice(&self, id: &str) -> Option<(&str, f32)> {
        match self.answers.get(id)? {
            Answer::Choice { key, probabilities } => {
                Some((key.as_str(), probabilities.get(key).copied().unwrap_or(0.0)))
            }
            _ => None,
        }
    }

    /// The position on the scale, from 0.
    pub fn score(&self, id: &str) -> Option<f32> {
        match self.answers.get(id)? {
            Answer::Score { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// Every answer in one line, for the log: `wants_brainstorm=0.03
    /// scope=small(0.91)`.
    pub fn summary(&self) -> String {
        self.answers
            .iter()
            .map(|(id, answer)| match answer {
                Answer::Bool { yes } => format!("{id}={yes:.2}"),
                Answer::Choice { key, probabilities } => format!(
                    "{id}={key}({:.2})",
                    probabilities.get(key).copied().unwrap_or(0.0)
                ),
                Answer::Score { value, .. } => format!("{id}={value:.2}"),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A decision model. `name` says which judgement this is, for a provider
/// that keeps per-use statistics; the answers depend only on the state and
/// the questions.
#[async_trait]
pub trait DecisionProvider: Send + Sync {
    /// `clef`, `jev`, `custom` or `llm`.
    fn id(&self) -> &str;
    fn model(&self) -> &str;
    async fn decide(&self, name: &str, state: &Value, questions: &[Question]) -> Result<Decision>;
}

/// The built-in providers and the ways to add one, in the order Settings
/// lists them.
pub const PROVIDERS: [&str; 4] = ["clef", "jev", "custom", "llm"];

/// TypeSafe's System One endpoint.
pub const JEV_URL: &str = "https://api.typesafe.ai/v1/systemone";

/// The models a built-in provider offers, its default first.
pub fn models(provider: &str) -> &'static [&'static str] {
    match provider {
        "clef" => &["@cf/cloudflare/clef-flash", "@cf/cloudflare/clef"],
        "jev" => &["jev-latest"],
        _ => &[],
    }
}

/// What the provider is called on screen.
pub fn provider_label(provider: &str) -> &'static str {
    match provider {
        "clef" => "Clef (Cloudflare Workers AI)",
        "jev" => "Jev (TypeSafe)",
        "custom" => "Custom endpoint (Jev/Clef API)",
        "llm" => "A model configured in enowx",
        _ => "unknown",
    }
}

/// The `auth.json` entry holding a provider's key.
pub fn secret_id(provider: &str) -> String {
    format!("decision-{provider}")
}

/// Environment variables a provider's key is read from when none is stored.
fn key_env(provider: &str) -> &'static [&'static str] {
    match provider {
        "clef" => &["CLOUDFLARE_API_TOKEN"],
        "jev" => &["TYPESAFE_API_KEY"],
        _ => &[],
    }
}

/// The `[decision]` table. Off unless `enabled`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DecisionConfig {
    pub enabled: bool,
    /// One of [`PROVIDERS`].
    pub provider: String,
    /// Blank: the provider's default, or for `llm` the model in use.
    pub model: String,
    /// Clef's Cloudflare account. `CLOUDFLARE_ACCOUNT_ID` is read when blank.
    pub account_id: String,
    /// A custom endpoint's URL.
    pub base_url: String,
    /// How long a judgement may take before the old path runs instead.
    pub timeout_ms: u64,
    /// Decide and record, but act as if no decision was made: for comparing
    /// before trusting.
    pub shadow: bool,
    pub uses: Uses,
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "clef".into(),
            model: String::new(),
            account_id: String::new(),
            base_url: String::new(),
            timeout_ms: 300,
            shadow: false,
            uses: Uses::default(),
        }
    }
}

impl DecisionConfig {
    /// The model to call: the configured one, else the provider's default.
    pub fn model(&self) -> String {
        if !self.model.trim().is_empty() {
            return self.model.trim().to_owned();
        }
        models(&self.provider)
            .first()
            .map(|m| (*m).to_owned())
            .unwrap_or_default()
    }
}

/// A judgement the harness can hand to the decision model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Use {
    Intent,
    Routing,
    Ask,
    ToolResults,
    Shell,
}

impl Use {
    pub const ALL: [Use; 5] = [
        Use::Intent,
        Use::Routing,
        Use::Ask,
        Use::ToolResults,
        Use::Shell,
    ];

    /// Its key in `[decision.uses]` and in the log.
    pub fn key(self) -> &'static str {
        match self {
            Use::Intent => "intent",
            Use::Routing => "routing",
            Use::Ask => "ask",
            Use::ToolResults => "tool_results",
            Use::Shell => "shell",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Use::Intent => "Brainstorm or build",
            Use::Routing => "Which specialist",
            Use::Ask => "When to ask the user",
            Use::ToolResults => "Keep tool results whole or cut",
            Use::Shell => "Risky shell commands",
        }
    }

    /// How sure the model has to be before its answer is acted on.
    pub fn default_threshold(self) -> f64 {
        match self {
            Use::Intent => 0.8,
            Use::Routing => 0.7,
            Use::Ask => 0.8,
            Use::ToolResults => 0.85,
            Use::Shell => 0.9,
        }
    }
}

/// One use: on or off, and its threshold.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UseConfig {
    pub enabled: bool,
    pub threshold: f64,
}

impl Default for UseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.8,
        }
    }
}

/// `[decision.uses]`: every use is on once the decision model is, each with
/// its own threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Uses {
    pub intent: UseConfig,
    pub routing: UseConfig,
    pub ask: UseConfig,
    pub tool_results: UseConfig,
    pub shell: UseConfig,
}

impl Default for Uses {
    fn default() -> Self {
        let on = |u: Use| UseConfig {
            enabled: true,
            threshold: u.default_threshold(),
        };
        Self {
            intent: on(Use::Intent),
            routing: on(Use::Routing),
            ask: on(Use::Ask),
            tool_results: on(Use::ToolResults),
            shell: on(Use::Shell),
        }
    }
}

impl Uses {
    pub fn get(&self, u: Use) -> UseConfig {
        match u {
            Use::Intent => self.intent,
            Use::Routing => self.routing,
            Use::Ask => self.ask,
            Use::ToolResults => self.tool_results,
            Use::Shell => self.shell,
        }
    }

    pub fn get_mut(&mut self, u: Use) -> &mut UseConfig {
        match u {
            Use::Intent => &mut self.intent,
            Use::Routing => &mut self.routing,
            Use::Ask => &mut self.ask,
            Use::ToolResults => &mut self.tool_results,
            Use::Shell => &mut self.shell,
        }
    }
}

/// The provider `config` describes, or why there is none: off, an unknown
/// provider, a missing key or account.
pub fn build_provider(config: &crate::config::Config) -> Result<Arc<dyn DecisionProvider>, String> {
    let decision = &config.decision;
    let provider = decision.provider.trim();
    let key = || {
        config
            .auth
            .key(&secret_id(provider), key_env(provider))
            .map(|(key, _)| key)
    };
    let model = decision.model();
    match provider {
        "clef" => {
            let account = if decision.account_id.trim().is_empty() {
                std::env::var("CLOUDFLARE_ACCOUNT_ID").unwrap_or_default()
            } else {
                decision.account_id.trim().to_owned()
            };
            if account.trim().is_empty() {
                return Err("Clef needs a Cloudflare account ID".into());
            }
            let key = key().ok_or("Clef needs a Cloudflare API token")?;
            Ok(Arc::new(JevCompat::clef(account.trim(), &key, &model)))
        }
        "jev" => {
            let key = key().ok_or("Jev needs a TypeSafe API key")?;
            Ok(Arc::new(JevCompat::jev(JEV_URL, &key, &model)))
        }
        "custom" => {
            let url = decision.base_url.trim();
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err("a custom endpoint needs its http(s) URL".into());
            }
            // A local endpoint may take no key.
            let key = key().unwrap_or_default();
            Ok(Arc::new(JevCompat::custom(url, &key, &model)))
        }
        "llm" => LlmJudge::from_config(config, &decision.model)
            .map(|judge| Arc::new(judge) as Arc<dyn DecisionProvider>),
        other => Err(format!("unknown decision provider `{other}`")),
    }
}

/// Ask the configured provider the smallest real question, with time to
/// spare, and say how it went: for Settings' connection test.
pub async fn test_connection(config: &crate::config::Config) -> Result<String, String> {
    let provider = build_provider(config)?;
    let questions = [Question::boolean("ok", "The state is the word ping.")];
    let limit = config.decision.timeout_ms;
    let started = Instant::now();
    let answer = tokio::time::timeout(
        Duration::from_secs(20),
        provider.decide("test", &json!("ping"), &questions),
    )
    .await;
    let ms = started.elapsed().as_millis() as u64;
    let name = if provider.model().is_empty() {
        provider.id()
    } else {
        provider.model()
    };
    match answer {
        Err(_) => Err("no answer within 20 s".into()),
        Ok(Err(error)) => Err(format!("{error:#}")),
        Ok(Ok(decision)) if decision.yes("ok").is_none() => {
            Err("it answered, but not in the shape expected".into())
        }
        Ok(Ok(_)) if ms > limit => Ok(format!(
            "works: {name} answered in {ms} ms, over the {limit} ms limit (raise it, or every \
             judgement falls back)"
        )),
        Ok(Ok(_)) => Ok(format!("works: {name} answered in {ms} ms")),
    }
}

/// What a use decided, and the record of it when the model was consulted.
#[derive(Debug, Clone)]
pub struct Ruling<T> {
    /// What to do. In shadow mode and on any failure, what runs without a
    /// decision model.
    pub verdict: T,
    pub record: Option<Record>,
}

impl<T> Ruling<T> {
    fn plain(verdict: T) -> Self {
        Self {
            verdict,
            record: None,
        }
    }
}

/// The decision model as the harness uses it: the provider, the settings,
/// the log. Cheap to clone.
#[derive(Clone)]
pub struct Decider {
    provider: Option<Arc<dyn DecisionProvider>>,
    config: DecisionConfig,
    log: DecisionLog,
}

impl Default for Decider {
    fn default() -> Self {
        Self::off()
    }
}

impl Decider {
    /// No decision model: every use takes the old path, nothing is logged.
    pub fn off() -> Self {
        Self {
            provider: None,
            config: DecisionConfig::default(),
            log: DecisionLog::memory(),
        }
    }

    /// The decision model `config` describes. Off when it is disabled or
    /// cannot be built; never fails.
    pub fn from_config(config: &crate::config::Config) -> Self {
        if !config.decision.enabled {
            return Self::off();
        }
        match build_provider(config) {
            Ok(provider) => Self {
                provider: Some(provider),
                config: config.decision.clone(),
                log: DecisionLog::file(),
            },
            Err(_) => Self::off(),
        }
    }

    /// A decider around any provider: tests, and hosts with their own.
    pub fn new(
        config: DecisionConfig,
        provider: Arc<dyn DecisionProvider>,
        log: DecisionLog,
    ) -> Self {
        Self {
            provider: Some(provider),
            config,
            log,
        }
    }

    /// Whether `u` consults the model.
    pub fn is_on(&self, u: Use) -> bool {
        self.provider.is_some() && self.config.uses.get(u).enabled
    }

    pub fn shadow(&self) -> bool {
        self.config.shadow
    }

    pub fn threshold(&self, u: Use) -> f32 {
        self.config.uses.get(u).threshold.clamp(0.5, 0.999) as f32
    }

    pub fn log(&self) -> &DecisionLog {
        &self.log
    }

    /// Put the questions to the provider within the time allowed. `Err`
    /// carries the record of a failure, already logged: the caller takes the
    /// old path.
    async fn consult(
        &self,
        u: Use,
        state: Value,
        questions: &[Question],
    ) -> Option<Result<Decision, Box<Record>>> {
        let provider = self.provider.as_ref().filter(|_| self.is_on(u))?;
        let started = Instant::now();
        let limit = Duration::from_millis(self.config.timeout_ms.max(1));
        let answer = tokio::time::timeout(limit, provider.decide(u.key(), &state, questions)).await;
        let ms = started.elapsed().as_millis() as u64;
        let failed = |outcome: &str, detail: String| {
            let record = Record::new(u, provider.id(), provider.model(), ms, outcome)
                .answer(detail)
                .action("old path");
            self.log.write(&record);
            Err(Box::new(record))
        };
        Some(match answer {
            Err(_) => failed(
                "timeout",
                format!("no answer within {} ms", limit.as_millis()),
            ),
            Ok(Err(error)) => failed("error", short(&format!("{error:#}"))),
            Ok(Ok(mut decision)) => {
                decision.latency_ms = ms;
                decision.provider = provider.id().to_owned();
                decision.model = provider.model().to_owned();
                Ok(decision)
            }
        })
    }

    /// Log a decision and what came of it. `acted` is whether the answer
    /// was confident enough to act on; shadow mode records it as `shadow`.
    fn conclude(
        &self,
        u: Use,
        decision: &Decision,
        probability: Option<f32>,
        acted: bool,
        action: impl Into<String>,
    ) -> Record {
        let outcome = match (acted, self.shadow()) {
            (true, true) => "shadow",
            (true, false) => "applied",
            (false, _) => "unsure",
        };
        let mut record = Record::new(
            u,
            &decision.provider,
            &decision.model,
            decision.latency_ms,
            outcome,
        )
        .answer(decision.summary())
        .action(action);
        record.probability = probability;
        self.log.write(&record);
        record
    }

    /// Log what the user did with a decision put to them, such as a shell
    /// command they were asked to confirm.
    pub fn record_final(&self, u: Use, action: impl Into<String>) -> Option<Record> {
        let provider = self.provider.as_ref()?;
        let record = Record::new(u, provider.id(), provider.model(), 0, "user").action(action);
        self.log.write(&record);
        Some(record)
    }
}

/// A one-line error, short enough for a log line.
fn short(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    line.chars().take(200).collect()
}
