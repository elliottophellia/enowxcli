//! The Jev API: TypeSafe's System One endpoint, and everything that speaks
//! it. Clef on Cloudflare Workers AI takes the same request and answers in
//! the same shape inside Workers AI's `result` envelope, and a custom
//! endpoint is any other server that does.
//!
//! A request is a state and a map of typed questions; the answer is a map of
//! typed answers under the same ids.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;
use serde_json::{json, Map, Value};

use super::{Answer, Decision, DecisionProvider, Kind, Question};

/// A client for one Jev-compatible endpoint.
pub struct JevCompat {
    http: reqwest::Client,
    id: &'static str,
    url: String,
    key: String,
    model: String,
    /// Whether the model goes in the body. Workers AI names it in the URL.
    model_in_body: bool,
}

impl JevCompat {
    fn build(id: &'static str, url: String, key: &str, model: &str, model_in_body: bool) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            id,
            url,
            key: key.to_owned(),
            model: model.to_owned(),
            model_in_body,
        }
    }

    /// Clef, or any model, on Cloudflare Workers AI.
    pub fn clef(account_id: &str, key: &str, model: &str) -> Self {
        Self::build(
            "clef",
            format!("https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/{model}"),
            key,
            model,
            false,
        )
    }

    /// TypeSafe's Jev.
    pub fn jev(url: &str, key: &str, model: &str) -> Self {
        Self::build("jev", url.to_owned(), key, model, true)
    }

    /// Another endpoint speaking the Jev API. An empty key sends none.
    pub fn custom(url: &str, key: &str, model: &str) -> Self {
        Self::build("custom", url.to_owned(), key, model, !model.is_empty())
    }

    /// Where requests go: for tests and the log.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The request body for `state` and `questions`.
    pub fn body(&self, state: &Value, questions: &[Question]) -> Value {
        let mut body = json!({
            "state": state,
            "questions": wire(questions),
        });
        if self.model_in_body {
            body["model"] = Value::String(self.model.clone());
        }
        body
    }
}

/// The questions as the API takes them.
fn wire(questions: &[Question]) -> Value {
    let mut map = Map::new();
    for question in questions {
        let value = match &question.kind {
            Kind::Bool => json!({
                "type": "noul",
                "instructions": question.instructions,
            }),
            Kind::Choice(options) => json!({
                "type": "choice",
                "instructions": question.instructions,
                "criteria": options
                    .iter()
                    .map(|(key, meaning)| (key.clone(), Value::String(meaning.clone())))
                    .collect::<Map<_, _>>(),
            }),
            Kind::Score(levels) => json!({
                "type": "score",
                "instructions": question.instructions,
                "criteria": levels,
            }),
        };
        map.insert(question.id.clone(), value);
    }
    Value::Object(map)
}

/// Read the answers to `questions` out of a response. Workers AI wraps the
/// answer in `result`; an answer missing or of the wrong type is left out,
/// never guessed.
pub fn parse(response: &Value, questions: &[Question]) -> BTreeMap<String, Answer> {
    let root = response.get("result").unwrap_or(response);
    let answers = root.get("answers").unwrap_or(root);
    let number = |v: &Value| v.as_f64().map(|n| n as f32);
    let mut out = BTreeMap::new();
    for question in questions {
        let Some(raw) = answers.get(&question.id) else {
            continue;
        };
        let answer = match &question.kind {
            Kind::Bool => raw
                .get("noul")
                .or_else(|| raw.get("probability"))
                .and_then(number)
                .or_else(|| number(raw))
                .map(|yes| Answer::Bool {
                    yes: yes.clamp(0.0, 1.0),
                }),
            Kind::Choice(options) => {
                let key = raw.get("choice").and_then(Value::as_str);
                key.filter(|key| options.iter().any(|(k, _)| k == key))
                    .map(|key| {
                        let mut probabilities: BTreeMap<String, f32> = raw
                            .get("probabilities")
                            .and_then(Value::as_object)
                            .map(|map| {
                                map.iter()
                                    .filter_map(|(k, v)| Some((k.clone(), number(v)?)))
                                    .collect()
                            })
                            .unwrap_or_default();
                        if !probabilities.contains_key(key) {
                            if let Some(confidence) = raw.get("confidence").and_then(number) {
                                probabilities.insert(key.to_owned(), confidence);
                            }
                        }
                        Answer::Choice {
                            key: key.to_owned(),
                            probabilities,
                        }
                    })
            }
            Kind::Score(_) => raw
                .get("score")
                .and_then(number)
                .map(|value| Answer::Score {
                    value,
                    confidence: raw.get("confidence").and_then(number).unwrap_or(0.0),
                }),
        };
        if let Some(answer) = answer {
            out.insert(question.id.clone(), answer);
        }
    }
    out
}

#[async_trait]
impl DecisionProvider for JevCompat {
    fn id(&self) -> &str {
        self.id
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn decide(&self, _name: &str, state: &Value, questions: &[Question]) -> Result<Decision> {
        let started = Instant::now();
        let mut request = self.http.post(&self.url).json(&self.body(state, questions));
        if !self.key.is_empty() {
            request = request.bearer_auth(&self.key);
        }
        let response = request
            .send()
            .await
            .context("could not reach the endpoint")?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            bail!("the key was rejected ({status})");
        }
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let detail: String = text.chars().take(160).collect();
            bail!("the endpoint answered {status}: {detail}");
        }
        let value: Value = serde_json::from_str(&text).context("the answer is not JSON")?;
        Ok(Decision {
            answers: parse(&value, questions),
            latency_ms: started.elapsed().as_millis() as u64,
            provider: self.id.into(),
            model: self.model.clone(),
            fallback_used: false,
        })
    }
}
