use std::{collections::BTreeMap, time::Duration};
mod models;
mod stream;
use models::parse_models;
pub use stream::SseDecoder;
use stream::{apply_frame, PartialCall};

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::agent_def::Tier;
use crate::{
    config::Config,
    message::{Message, ToolCall},
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub models_url: &'static str,
    pub key_url: &'static str,
}
pub const PROVIDER_PRESETS: [ProviderPreset; 6] = [
    ProviderPreset {
        id: "enxapi",
        name: "enxapi",
        base_url: "https://enxapi.id/v1",
        models_url: "https://enxapi.id/v1/models",
        key_url: "https://enxapi.id",
    },
    ProviderPreset {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        models_url: "https://api.openai.com/v1/models",
        key_url: "https://platform.openai.com/api-keys",
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        models_url: "https://openrouter.ai/api/v1/models",
        key_url: "https://openrouter.ai/settings/keys",
    },
    ProviderPreset {
        id: "groq",
        name: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        models_url: "https://api.groq.com/openai/v1/models",
        key_url: "https://console.groq.com/keys",
    },
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        base_url: "https://api.deepseek.com",
        models_url: "https://api.deepseek.com/models",
        key_url: "https://platform.deepseek.com/api_keys",
    },
    ProviderPreset {
        id: "custom",
        name: "Custom OpenAI-compatible",
        base_url: "",
        models_url: "",
        key_url: "",
    },
];

pub fn provider_preset(id: &str) -> Option<ProviderPreset> {
    PROVIDER_PRESETS
        .iter()
        .copied()
        .find(|preset| preset.id == id)
}

#[derive(Debug, Default)]
pub struct Completion {
    pub text: String,
    pub reasoning: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
    pub usage: Usage,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug)]
pub enum Chunk {
    Text(String),
    Reasoning(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: Option<String>,
    pub context_window: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

impl ModelInfo {
    /// One line of detected facts. Absent values read as `unknown` rather than
    /// a guessed number, so nothing looks detected that the endpoint never sent.
    pub fn summary(&self) -> String {
        let context = self
            .context_window
            .map_or_else(|| "unknown".into(), |n| n.to_string());
        let output = self
            .max_output_tokens
            .map_or_else(|| "unknown".into(), |n| n.to_string());
        let mut summary = String::new();
        if let Some(name) = self.name.as_deref().filter(|name| *name != self.id) {
            summary.push_str(name);
            summary.push_str(" · ");
        }
        summary.push_str(&format!("context {context} · output {output}"));
        summary
    }
}

pub struct Provider {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    temperature: Option<f32>,
    active: bool,
}

impl Provider {
    pub fn from_config(config: &Config) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(600))
                .build()?,
            base_url: config.provider.base_url.trim_end_matches('/').to_owned(),
            api_key: config.provider.api_key.clone(),
            model: config.model.default.clone(),
            temperature: config.model.temperature,
            active: config.provider_active(),
        })
    }

    /// Detect model metadata from the exact URL supplied by the user.
    pub async fn models(&self, url: &str) -> Result<Vec<ModelInfo>> {
        anyhow::ensure!(
            self.active,
            "Set an active provider before detecting models"
        );
        let url = reqwest::Url::parse(url.trim()).context("Invalid model-list URL")?;
        anyhow::ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none(),
            "Model-list URL must be HTTP(S) without embedded credentials"
        );
        let base = reqwest::Url::parse(&self.base_url).context("Invalid provider base URL")?;
        anyhow::ensure!(
            matches!(base.scheme(), "http" | "https")
                && base.host_str().is_some()
                && base.username().is_empty()
                && base.password().is_none(),
            "Invalid provider base URL"
        );
        // A user-supplied catalogue may be hosted elsewhere; never send it the provider key.
        let same_origin = url.origin() == base.origin();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()?;
        let mut request = client.get(url);
        if same_origin && !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        let response = request.send().await.context("Fetching provider models")?;
        anyhow::ensure!(
            response.status().is_success(),
            "Model discovery returned {}",
            response.status()
        );
        let value: Value = response
            .json()
            .await
            .context("Model-list URL must return JSON")?;
        parse_models(&value)
    }
}

/// Callback the retry loop uses to tell the UI a transient failure is being
/// retried. Receives the failure text plus which attempt this is, so the UI
/// can render one collapsing line rather than one message per attempt.
/// `None` disables notifications.
pub type NoticeSink = Option<Box<dyn Fn(String, u32, u32) + Send + Sync>>;

/// Callback the tier ladder uses to announce a downgrade.
///
/// Deliberately a second sink rather than a third arm on `NoticeSink`: a
/// retry notice collapses into one updating line and disappears, while a tier
/// drop must stay in the transcript. `docs/agents.md` is explicit that this
/// notice is not optional — a cheaper model may not be up to the task, and a
/// silent downgrade produces worse work with no indication why.
pub type TierNoticeSink = Option<Box<dyn Fn(TierDrop) + Send + Sync>>;

/// One rung of the ladder being taken, with enough detail to render the
/// sentence the doc asks for: *"strong unavailable, continuing with balanced"*.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierDrop {
    /// Tier that just ran out of models, `None` when the move is sideways
    /// within a tier (a second model at the same level).
    pub from_tier: Option<Tier>,
    /// Tier being moved to.
    pub to_tier: Tier,
    /// Model id about to be tried.
    pub model: String,
    /// Why the previous model was abandoned, for the transcript.
    pub reason: String,
}

impl TierDrop {
    /// The transcript line. Sideways moves inside a tier read differently
    /// from a drop, because only a drop costs the user capability.
    pub fn message(&self) -> String {
        match self.from_tier {
            Some(from) if from != self.to_tier => format!(
                "{} unavailable, continuing with {} ({})",
                from.id(),
                self.to_tier.id(),
                self.model
            ),
            _ => format!("retrying on {} ({})", self.model, self.to_tier.id()),
        }
    }
}

impl Provider {
    pub async fn complete(
        &self,
        messages: &[Message],
        tools: &[Value],
        sink: &mpsc::Sender<Chunk>,
    ) -> Result<Completion> {
        self.complete_with_notice(messages, tools, sink, &None)
            .await
    }

    /// Same as `complete` but calls `notice` on every retry so the UI can
    /// show progress. Retries transient failures (network, 5xx, 429, stream
    /// disconnect) up to `MAX_RETRIES` with exponential backoff.
    pub async fn complete_with_notice(
        &self,
        messages: &[Message],
        tools: &[Value],
        sink: &mpsc::Sender<Chunk>,
        notice: &NoticeSink,
    ) -> Result<Completion> {
        // Start at the widest budget; the first failure narrows it. Nothing is
        // retried before a failure, so an optimistic cap costs nothing. The
        // budget is only knowable after a failure — it depends on the kind and
        // on whether the stream had started — which is why this is a loop over
        // a mutable cap rather than a fixed `1..=MAX_RETRIES` range.
        let mut budget = MAX_RETRIES;
        let mut attempt = 1;
        loop {
            match self.complete_once(messages, tools, sink).await {
                Ok(c) => return Ok(c),
                Err(e) => {
                    // Re-read the budget every time and only ever narrow it: a
                    // call can fail on transport before the stream starts and
                    // then fail mid-stream on the retry, which drops the cap
                    // to 2 and must not be raised back to 10.
                    budget = budget.min(retry_budget_for_error(&e));
                    if attempt >= budget {
                        return Err(e);
                    }
                    let delay = backoff_ms(attempt);
                    if let Some(cb) = notice {
                        // Hand over the parts, not a sentence: the UI shows a
                        // single line that updates in place, so it needs the
                        // attempt number separately from the message.
                        cb(format!("{e}"), attempt + 1, budget);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn complete_once(
        &self,
        messages: &[Message],
        tools: &[Value],
        sink: &mpsc::Sender<Chunk>,
    ) -> Result<Completion> {
        let mut payload = json!({
            "model": self.model,
            "messages": messages.iter().map(Message::to_wire).collect::<Vec<_>>(),
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        if let Some(temperature) = self.temperature {
            payload["temperature"] = json!(temperature);
        }
        if !tools.is_empty() {
            payload["tools"] = json!(tools);
            payload["tool_choice"] = json!("auto");
        }
        let mut request = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .json(&payload);
        if !self.api_key.is_empty() {
            request = request.bearer_auth(&self.api_key);
        }
        let response = request.send().await.context("calling the provider")?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.context("reading provider error")?;
            bail!(
                "provider returned {status}: {}",
                body.chars().take(2000).collect::<String>()
            );
        }
        // A base URL pointing at a site rather than an API answers 200 with
        // an HTML page, which decodes to zero SSE frames and surfaced as
        // "stream ended without a finish reason" — an error about the model
        // for what is actually a configuration mistake. Name it instead.
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        if content_type.contains("text/html") {
            bail!(
                "provider returned an HTML page, not a stream. `{}` does not look \
                 like an API endpoint — check provider.base_url (an OpenAI-compatible \
                 base usually ends in /v1).",
                self.base_url
            );
        }

        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::default();
        let mut result = Completion::default();
        let mut calls: BTreeMap<usize, PartialCall> = BTreeMap::new();
        let mut done = false;
        // The retry budget shrinks once work has been billed, so the loop has
        // to know whether anything arrived before the break. Nothing tracked
        // that before: a failure on byte one and a failure after 4,000
        // streamed tokens produced indistinguishable errors, and both got the
        // full ten attempts.
        fn started(result: &Completion, calls: &BTreeMap<usize, PartialCall>) -> bool {
            !result.text.is_empty() || !result.reasoning.is_empty() || !calls.is_empty()
        }
        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Err(mark_mid_stream(
                        anyhow::Error::new(e).context("reading provider stream"),
                        started(&result, &calls),
                    ))
                }
            };
            let frames = match decoder.push(&bytes) {
                Ok(frames) => frames,
                Err(e) => return Err(mark_mid_stream(e, started(&result, &calls))),
            };
            for data in frames {
                if data == "[DONE]" {
                    done = true;
                    break;
                }
                if let Err(e) = apply_frame(&data, &mut result, &mut calls, sink).await {
                    return Err(mark_mid_stream(e, started(&result, &calls)));
                }
            }
            if done {
                break;
            }
        }
        if result.finish_reason.is_empty() {
            // Distinguish "the provider said nothing at all" from "it replied
            // but never terminated the stream": the first is almost always a
            // wrong endpoint, the second a genuine upstream fault.
            if result.text.is_empty() && calls.is_empty() {
                bail!(
                    "provider sent no data. Check provider.base_url (`{}`) and the API key.",
                    self.base_url
                );
            }
            // Content arrived but the stream never terminated: those tokens
            // were produced and billed, so this is the expensive case the
            // mid-stream cap exists for.
            return Err(mark_mid_stream(
                anyhow::anyhow!("provider stream ended without a finish reason; no tools executed"),
                true,
            ));
        }
        for (_, call) in calls {
            if call.id.is_empty() || call.name.is_empty() {
                bail!("provider returned an incomplete tool call");
            }
            result.tool_calls.push(ToolCall {
                id: call.id,
                name: call.name,
                arguments: call.arguments,
            });
        }
        Ok(result)
    }
}

/// Hard cap on how many times a single completion is retried. Ten attempts
/// with the backoff schedule below tops out around 30 seconds of waiting
/// before giving up, which covers most transient upstream issues without
/// making the user sit forever.
///
/// This is the budget for `Capacity` and `Transport` only. The other kinds
/// get their own number — see `FailureKind::retry_budget`.
const MAX_RETRIES: u32 = 10;

/// Attempts allowed when a stream dies *after* the response started.
///
/// Two, not ten, and the difference is money rather than taste: a 429 is
/// refused before the model processes anything, so retrying it costs time
/// alone. A stream that has already emitted tokens was processed and billed,
/// and every retry pays for that prompt again. Ten attempts on a long prompt
/// is ten full charges for one answer.
const MAX_MID_STREAM_RETRIES: u32 = 2;

/// Marker appended to a stream error once content has been emitted.
///
/// The classification happens on the error string, and the string is all that
/// survives `anyhow`'s context chain, so the "did anything arrive?" fact has
/// to travel in it. Unlikely enough to appear in an upstream message that a
/// false positive is not a real concern.
const MID_STREAM_MARKER: &str = "[mid-stream]";

fn backoff_ms(attempt: u32) -> u64 {
    // 200, 400, 800, 1600, 3200, 6400, 8000, 8000, 8000
    let base = 200u64 << attempt.min(6);
    base.min(8_000)
}

/// The four ways a model call fails, per the table in `docs/agents.md`.
///
/// They are separated because the two useful questions — "does retrying
/// help?" and "does another model help?" — have different answers for each,
/// and collapsing them loses one answer or the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Connection reset, timeout, broken pipe, EOF. Retry helps; another
    /// model does not, because the network is what broke.
    Transport,
    /// 429, 503, overloaded, service unavailable, bad gateway. Both retrying
    /// and moving to another model help.
    Capacity,
    /// Context overflow, too many tokens, or a stream that produced nothing
    /// usable. Retrying cannot help — the same request will overflow the same
    /// window every time — but a different model can.
    Capability,
    /// 401, 403, 404, unknown model, malformed request, misconfiguration.
    /// Nothing helps; report it now rather than after 30 seconds of backoff.
    Permanent,
}

impl FailureKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Transport => "transport",
            Self::Capacity => "capacity",
            Self::Capability => "capability",
            Self::Permanent => "permanent",
        }
    }

    /// Total attempts allowed against one model for this kind of failure.
    ///
    /// DO NOT flatten these to a single constant. The numbers differ for a
    /// reason recorded in `docs/agents.md`:
    ///
    /// - `Capacity`/`Transport` get 10 because a rejected request is refused
    ///   before processing, so it is generally not billed. Only time is
    ///   spent, and waiting out a rate limit is exactly what the caller
    ///   wants.
    /// - `Capability` and `Permanent` get 1 (the first attempt, no retries)
    ///   because the failure is deterministic. A context overflow overflows
    ///   identically on attempt ten; a 401 is still a 401. Retrying only
    ///   makes the user wait for the same answer.
    ///
    /// The mid-stream case is budgeted separately — see
    /// `retry_budget_for_error`, which cannot be expressed here because it
    /// depends on how far the call got, not on which kind it was.
    pub fn retry_budget(self) -> u32 {
        match self {
            Self::Capacity | Self::Transport => MAX_RETRIES,
            Self::Capability | Self::Permanent => 1,
        }
    }

    /// Whether dropping to a different model could plausibly succeed. The
    /// ladder consults this: retrying a `Permanent` failure on a cheaper
    /// model just produces the same 401 with a worse model named in it.
    pub fn another_model_helps(self) -> bool {
        matches!(self, Self::Capacity | Self::Capability)
    }
}

/// Classify a failure from its message.
///
/// Order matters. Misconfiguration is checked first because those messages
/// name the endpoint and the word "connection" appears in some of them, which
/// would otherwise read as `Transport`. Capability is checked before capacity
/// so a context-overflow 400 is not mistaken for a generic client error.
pub fn classify(err: &anyhow::Error) -> FailureKind {
    classify_message(&format!("{err:#}"))
}

fn classify_message(raw: &str) -> FailureKind {
    let msg = raw.to_ascii_lowercase();

    // Misconfiguration never fixes itself, so retrying it just delays the
    // report by the whole backoff schedule while repeating the same line.
    if msg.contains("not a stream")
        || msg.contains("provider sent no data")
        || msg.contains("check provider.base_url")
    {
        return FailureKind::Permanent;
    }

    // Capability: the request cannot be served by this model as written.
    // "no finish reason" lives here rather than under transport: the stream
    // completed without producing anything usable, and a second identical
    // request to the same model usually does the same thing. Another model
    // is the fix, which is what capability means.
    if msg.contains("context_length_exceeded")
        || msg.contains("context length")
        || msg.contains("context window")
        || msg.contains("context overflow")
        || msg.contains("too many tokens")
        || msg.contains("maximum context")
        || msg.contains("reduce the length")
        || msg.contains("finish reason")
    {
        return FailureKind::Capability;
    }

    // Capacity: the server refused before doing the work.
    if msg.contains("provider returned 429")
        || msg.contains("provider returned 503")
        || msg.contains("provider returned 502")
        || msg.contains("rate limit")
        || msg.contains("overloaded")
        || msg.contains("service unavailable")
        || msg.contains("bad gateway")
        || msg.contains("capacity")
    {
        return FailureKind::Capacity;
    }

    // Permanent: auth, missing route, bad request. Checked before transport
    // so a "connection refused by policy" style 403 is not read as a network
    // fault.
    if msg.contains("provider returned 401")
        || msg.contains("provider returned 403")
        || msg.contains("provider returned 404")
        || msg.contains("provider returned 400")
        || msg.contains("unknown model")
        || msg.contains("model not found")
        || msg.contains("invalid api key")
        || msg.contains("incomplete tool call")
        || msg.contains("malformed")
    {
        return FailureKind::Permanent;
    }

    // Transport: the connection itself failed.
    if msg.contains("timeout")
        || msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("reset")
        || msg.contains("broken pipe")
        || msg.contains("eof")
        || msg.contains("provider returned 5")
        || msg.contains("provider returned 408")
        || msg.contains("gateway timeout")
    {
        return FailureKind::Transport;
    }

    // Anything unrecognised is treated as permanent. Retrying an error we
    // cannot name ten times is the worse default: it burns 30 seconds to
    // reach the same report.
    FailureKind::Permanent
}

/// Attempts allowed for this specific error, which is the kind's budget
/// except when the stream had already started.
///
/// Split from `FailureKind::retry_budget` because "how far did it get" is not
/// a property of the kind. A transport failure before the first byte and a
/// transport failure after 4,000 tokens are the same kind and cost very
/// different amounts.
pub fn retry_budget_for_error(err: &anyhow::Error) -> u32 {
    let kind = classify(err);
    let budget = kind.retry_budget();
    if started_streaming(err) {
        // The work was processed and billed; cap it regardless of kind. Never
        // raise a kind's budget — a capability failure that got halfway is
        // still not worth a second identical request.
        return budget.min(MAX_MID_STREAM_RETRIES);
    }
    budget
}

/// Whether this failure happened after the response had started arriving.
fn started_streaming(err: &anyhow::Error) -> bool {
    format!("{err:#}").contains(MID_STREAM_MARKER)
}

/// Tag a stream failure with how far it got, when it got anywhere.
///
/// The marker rides in the message because that is the only channel that
/// survives the `anyhow` context chain intact all the way to the retry loop.
fn mark_mid_stream(err: anyhow::Error, started: bool) -> anyhow::Error {
    if started {
        err.context(MID_STREAM_MARKER)
    } else {
        err
    }
}

/// True when an error is worth retrying at all against the same model.
///
/// Kept as a thin wrapper over `classify` so the meaning stays in one place,
/// and kept public because "is this worth waiting on?" is a question callers
/// outside the retry loop also ask.
pub fn is_transient(err: &anyhow::Error) -> bool {
    matches!(
        classify(err),
        FailureKind::Transport | FailureKind::Capacity
    )
}

/// One model to try, and the tier it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LadderStep {
    pub model: String,
    pub tier: Tier,
}

/// Walks the fallback ladder from `docs/agents.md`: another model in the same
/// tier, then one tier down, then give up naming everything tried.
///
/// Holds the list of what has been attempted so the final error can name it.
/// A failure that reports only the last model reads as "the cheap model is
/// broken" when in fact every tier had been exhausted.
#[derive(Debug, Clone)]
pub struct ModelLadder {
    agent: String,
    tier: Tier,
    tried: Vec<String>,
}

impl ModelLadder {
    /// Start at the agent's own tier with the model it is already using.
    ///
    /// `current` is recorded as tried straight away: the ladder is built
    /// after that model has failed, and offering it again would loop.
    pub fn new(agent: impl Into<String>, tier: Tier, current: impl Into<String>) -> Self {
        let current = current.into();
        let tried = if current.trim().is_empty() {
            Vec::new()
        } else {
            vec![current]
        };
        Self {
            agent: agent.into(),
            tier,
            tried,
        }
    }

    /// Models already attempted, in order.
    pub fn tried(&self) -> &[String] {
        &self.tried
    }

    /// The tier currently being tried.
    pub fn tier(&self) -> Tier {
        self.tier
    }

    /// The next model to try, or `None` at the bottom of the ladder.
    ///
    /// Each returned step is recorded, so a config where two tiers resolve to
    /// the same model id skips the duplicate rather than retrying it under a
    /// new label — the user would otherwise see "dropping to cheap" and get
    /// exactly the model that just failed.
    pub fn next(&mut self, config: &Config) -> Option<LadderStep> {
        loop {
            // Step 2 of the ladder: another model at this level. Only yields
            // once per tier, because `model_for` resolves to one id — a
            // second identical answer means the tier is out of options.
            let same_tier = config.model_for(&self.agent, self.tier);
            if !same_tier.trim().is_empty() && !self.already_tried(&same_tier) {
                self.tried.push(same_tier.clone());
                return Some(LadderStep {
                    model: same_tier,
                    tier: self.tier,
                });
            }
            // Step 3: drop a tier. Step 4 is this returning None.
            self.tier = self.tier.lower()?;
        }
    }

    /// The give-up error, naming every model tried so the report says which
    /// options were exhausted rather than only the last one.
    pub fn exhausted(&self, cause: &anyhow::Error) -> anyhow::Error {
        let tried = if self.tried.is_empty() {
            "none".to_owned()
        } else {
            self.tried.join(", ")
        };
        anyhow::anyhow!(
            "every model tier failed for agent `{}`; tried: {tried}. Last error: {cause:#}",
            self.agent
        )
    }

    fn already_tried(&self, model: &str) -> bool {
        self.tried.iter().any(|seen| seen == model)
    }
}

/// Build the announcement for moving to `step` after `previous_tier`.
///
/// Separate from `ModelLadder::next` so the caller decides when to emit it —
/// but `docs/agents.md` is explicit that it must be emitted, not that it is
/// optional.
pub fn tier_drop(previous_tier: Tier, step: &LadderStep, reason: &anyhow::Error) -> TierDrop {
    TierDrop {
        from_tier: Some(previous_tier),
        to_tier: step.tier,
        model: step.model.clone(),
        reason: format!("{reason}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_fragments_preserve_unicode_and_multiline_events() {
        let bytes = "data: héllo\r\ndata: 世界\r\n\r\n".as_bytes();
        let mut decoder = SseDecoder::default();
        let mut frames = Vec::new();
        for byte in bytes {
            frames.extend(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(frames, ["héllo\n世界"]);
    }

    /// Catalogues differ per provider: ids may sit under `data`, `models`, or a
    /// bare array, and limits arrive under several names. Absent limits stay
    /// `None` so the UI never presents a guessed context window as detected.
    #[test]
    fn model_catalogues_expose_available_limits_only() {
        let openai_style = serde_json::json!({"data":[
            {"id":"vendor/b","context_length":128000,"top_provider":{"max_completion_tokens":8192}},
            {"id":"vendor/a"},
            {"id":"  "},
        ]});
        let models = parse_models(&openai_style).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "vendor/a");
        assert_eq!(models[0].context_window, None);
        assert_eq!(models[1].context_window, Some(128_000));
        assert_eq!(models[1].max_output_tokens, Some(8192));

        let gemini_style = serde_json::json!({"models":[
            {"name":"models/gemini","displayName":"Gemini","inputTokenLimit":"1048576","outputTokenLimit":65536},
        ]});
        let models = parse_models(&gemini_style).unwrap();
        assert_eq!(models[0].context_window, Some(1_048_576));
        assert_eq!(models[0].max_output_tokens, Some(65_536));

        assert!(parse_models(&serde_json::json!({"data":[]})).is_err());
        assert!(parse_models(&serde_json::json!({"error":"nope"})).is_err());
    }
    #[tokio::test]
    async fn interleaved_tool_arguments_remain_paired() {
        let (tx, _rx) = mpsc::channel(8);
        let mut result = Completion::default();
        let mut calls = BTreeMap::new();
        for data in [
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"id":"b","function":{"name":"read","arguments":"{\"path\":"}},{"index":0,"id":"a","function":{"name":"glob","arguments":"{\"pattern\":\"*.rs\"}"}}]}}]}"#,
            r#"{"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"\"lib.rs\"}"}}]},"finish_reason":"tool_calls"}]}"#,
        ] {
            apply_frame(data, &mut result, &mut calls, &tx)
                .await
                .unwrap();
        }
        assert_eq!(calls[&1].arguments, r#"{"path":"lib.rs"}"#);
        assert_eq!(calls[&0].arguments, r#"{"pattern":"*.rs"}"#);
        assert!(apply_frame(
            r#"{"error":{"message":"rate limit"}}"#,
            &mut result,
            &mut calls,
            &tx
        )
        .await
        .is_err());
    }
}

#[cfg(test)]
mod diagnosis_tests {
    use super::*;

    /// A wrong base URL is permanent: retrying it ten times only delays the
    /// report and repeats the same line while the user waits out the backoff.
    #[test]
    fn configuration_errors_are_not_retried() {
        for message in [
            "provider returned an HTML page, not a stream. `https://ai.example.id` does not look like an API endpoint",
            "provider sent no data. Check provider.base_url (`https://ai.example.id`) and the API key.",
        ] {
            assert!(
                !is_transient(&anyhow::anyhow!(message.to_string())),
                "must not retry: {message}"
            );
        }
    }

    /// Genuine upstream faults still retry.
    #[test]
    fn upstream_faults_are_still_transient() {
        for message in [
            "connection reset by peer",
            "provider returned 503 service unavailable",
            "operation timed out",
        ] {
            assert!(
                is_transient(&anyhow::anyhow!(message.to_string())),
                "should retry: {message}"
            );
        }
    }

    /// Auth and bad-request failures were never retried; keep it that way.
    #[test]
    fn client_errors_are_not_retried() {
        for message in [
            "provider returned 401: invalid api key",
            "provider returned 404: model not found",
        ] {
            assert!(
                !is_transient(&anyhow::anyhow!(message.to_string())),
                "must not retry: {message}"
            );
        }
    }

    fn err(message: &str) -> anyhow::Error {
        anyhow::anyhow!(message.to_string())
    }

    /// The doc's table, one row at a time. These are the inputs the rest of
    /// the ladder reasons about, so a misclassification here silently changes
    /// every budget downstream.
    #[test]
    fn each_failure_lands_in_its_own_kind() {
        for message in [
            "connection reset by peer",
            "operation timed out",
            "broken pipe",
            "unexpected eof during body read",
            "provider returned 500: internal",
        ] {
            assert_eq!(
                classify(&err(message)),
                FailureKind::Transport,
                "transport: {message}"
            );
        }
        for message in [
            "provider returned 429: rate limit exceeded",
            "provider returned 503: service unavailable",
            "provider returned 502: bad gateway",
            "the model is currently overloaded",
        ] {
            assert_eq!(
                classify(&err(message)),
                FailureKind::Capacity,
                "capacity: {message}"
            );
        }
        for message in [
            "provider returned 400: context_length_exceeded",
            "this request exceeds the maximum context length",
            "too many tokens in the prompt",
            "provider stream ended without a finish reason; no tools executed",
        ] {
            assert_eq!(
                classify(&err(message)),
                FailureKind::Capability,
                "capability: {message}"
            );
        }
        for message in [
            "provider returned 401: invalid api key",
            "provider returned 403: forbidden",
            "provider returned 404: model not found",
            "unknown model vendor/nope",
            "malformed request body",
            "provider sent no data. Check provider.base_url (`https://x`) and the API key.",
            "provider returned an HTML page, not a stream. `https://x` does not look like an API endpoint",
        ] {
            assert_eq!(
                classify(&err(message)),
                FailureKind::Permanent,
                "permanent: {message}"
            );
        }
    }

    /// The budgets are the whole point of the taxonomy. If someone flattens
    /// them back to one number this is the test that says no.
    #[test]
    fn each_kind_gets_the_budget_the_doc_specifies() {
        assert_eq!(FailureKind::Capacity.retry_budget(), 10);
        assert_eq!(FailureKind::Transport.retry_budget(), 10);
        assert_eq!(
            FailureKind::Capability.retry_budget(),
            1,
            "retrying cannot fix a context overflow"
        );
        assert_eq!(
            FailureKind::Permanent.retry_budget(),
            1,
            "a 401 is still a 401 on attempt ten"
        );
    }

    /// The expensive case: tokens were produced and billed before the stream
    /// broke, so every retry pays for the prompt again. Two attempts, not ten.
    #[test]
    fn a_post_response_stream_failure_gets_two_attempts_not_ten() {
        let fresh = err("connection reset by peer");
        assert_eq!(
            retry_budget_for_error(&fresh),
            10,
            "nothing was billed before the first byte"
        );

        let mid = mark_mid_stream(err("connection reset by peer"), true);
        assert_eq!(
            retry_budget_for_error(&mid),
            2,
            "billed work must not be retried ten times"
        );
        assert_eq!(
            classify(&mid),
            FailureKind::Transport,
            "the marker must not change what kind it is"
        );
    }

    /// The marker only ever narrows. A capability failure that got halfway is
    /// still not worth a second identical request.
    #[test]
    fn the_mid_stream_cap_never_raises_a_budget() {
        let mid = mark_mid_stream(err("context_length_exceeded"), true);
        assert_eq!(retry_budget_for_error(&mid), 1);
        assert!(!mark_mid_stream(err("boom"), false)
            .to_string()
            .contains("mid-stream"));
    }

    /// Only capacity and capability are worth carrying to another model: a
    /// 401 reproduced on a cheaper model is the same 401.
    #[test]
    fn only_some_kinds_are_worth_another_model() {
        assert!(FailureKind::Capacity.another_model_helps());
        assert!(FailureKind::Capability.another_model_helps());
        assert!(!FailureKind::Transport.another_model_helps());
        assert!(!FailureKind::Permanent.another_model_helps());
    }

    fn ladder_config() -> Config {
        let mut config = Config::default();
        config.model.default = "active/model".into();
        config.agent.tiers.strong = "tier/strong".into();
        config.agent.tiers.balanced = "tier/balanced".into();
        config.agent.tiers.cheap = "tier/cheap".into();
        config
    }

    /// The ladder walks down and stops at the bottom rather than looping on
    /// `cheap` forever.
    #[test]
    fn the_ladder_walks_down_and_stops_at_the_bottom() {
        let config = ladder_config();
        let mut ladder = ModelLadder::new("be", Tier::Strong, "tier/strong");

        let balanced = ladder.next(&config).expect("balanced rung");
        assert_eq!(balanced.model, "tier/balanced");
        assert_eq!(balanced.tier, Tier::Balanced);

        let cheap = ladder.next(&config).expect("cheap rung");
        assert_eq!(cheap.model, "tier/cheap");
        assert_eq!(cheap.tier, Tier::Cheap);

        assert!(ladder.next(&config).is_none(), "cheap is the floor");
        assert!(ladder.next(&config).is_none(), "and stays the floor");
    }

    /// A per-agent override is a different model at the same tier, which is
    /// step 2 of the doc's ladder and must be tried before dropping.
    #[test]
    fn a_same_tier_model_is_tried_before_dropping_a_tier() {
        let mut config = ladder_config();
        config
            .agent
            .models
            .insert("fe".into(), "override/fe".into());
        // `fe` is already running on the tier model, so the override is the
        // other model at this level.
        let mut ladder = ModelLadder::new("fe", Tier::Strong, "tier/strong");

        let step = ladder.next(&config).expect("same-tier rung");
        assert_eq!(step.model, "override/fe");
        assert_eq!(step.tier, Tier::Strong, "still strong, not a downgrade");
    }

    /// Two tiers pointing at one model id must not be offered twice: the user
    /// would be told work dropped to `cheap` and get the model that failed.
    #[test]
    fn a_duplicate_model_id_is_not_offered_again() {
        let mut config = ladder_config();
        config.agent.tiers.balanced = "tier/strong".into();
        let mut ladder = ModelLadder::new("be", Tier::Strong, "tier/strong");

        let step = ladder.next(&config).expect("skips the duplicate");
        assert_eq!(step.model, "tier/cheap");
        assert_eq!(step.tier, Tier::Cheap);
    }

    /// The give-up error names everything tried. Reporting only the last one
    /// reads as "the cheap model is broken" when every tier had failed.
    #[test]
    fn the_exhausted_error_names_every_model_tried() {
        let config = ladder_config();
        let mut ladder = ModelLadder::new("be", Tier::Strong, "tier/strong");
        while ladder.next(&config).is_some() {}

        let message = format!("{:#}", ladder.exhausted(&err("provider returned 429")));
        for model in ["tier/strong", "tier/balanced", "tier/cheap"] {
            assert!(message.contains(model), "{message} must name {model}");
        }
        assert!(message.contains("be"), "and the agent it was for");
        assert!(message.contains("429"), "and the cause");
    }

    /// The downgrade sentence the doc asks for verbatim. A sideways move
    /// inside a tier reads differently, because only a drop costs capability.
    #[test]
    fn a_tier_drop_announces_itself() {
        let step = LadderStep {
            model: "tier/balanced".into(),
            tier: Tier::Balanced,
        };
        let drop = tier_drop(Tier::Strong, &step, &err("provider returned 429"));
        assert_eq!(
            drop.message(),
            "strong unavailable, continuing with balanced (tier/balanced)"
        );

        let sideways = TierDrop {
            from_tier: Some(Tier::Strong),
            to_tier: Tier::Strong,
            model: "override/fe".into(),
            reason: "boom".into(),
        };
        assert_eq!(sideways.message(), "retrying on override/fe (strong)");
    }
}
