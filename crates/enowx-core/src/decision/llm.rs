//! A chat model configured in enowx, standing in as a decision model.
//!
//! It is asked for a JSON object, one answer and a confidence per question,
//! and its own confidence stands in for a probability. Slower and less
//! calibrated than a model built for the job, but it needs nothing new: any
//! provider already connected works.

use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{Answer, Decision, DecisionProvider, Kind, Question};
use crate::message::Message;
use crate::provider::Provider;

const SYSTEM: &str = "You answer typed questions about a state, for software that acts on \
the answers. Reply with one JSON object and nothing else: no prose, no code fence. For each \
question id give {\"answer\": ..., \"confidence\": a number from 0 to 1}. A yes/no question's \
answer is true or false. A choice question's answer is one of its keys, exactly as written. A \
score question's answer is the number of its level, counting from 0. Confidence is how sure \
you are; give less than 0.6 when the state does not settle the question.";

pub struct LlmJudge {
    provider: Provider,
    model: String,
}

impl LlmJudge {
    /// The model `model` names (`provider/model`), or the one in use when
    /// it is blank.
    pub fn from_config(config: &crate::config::Config, model: &str) -> Result<Self, String> {
        let mut config = config.clone();
        let model = model.trim();
        if !model.is_empty() && !config.use_model(model) {
            return Err(format!("`{model}` is not a model on a connected provider"));
        }
        if !config.is_ready() {
            return Err("no model in use to answer with".into());
        }
        let provider = Provider::from_config(&config).map_err(|e| format!("{e:#}"))?;
        Ok(Self {
            provider,
            model: config.model.active.clone(),
        })
    }
}

/// The questions, written out for a chat model.
pub fn prompt(state: &Value, questions: &[Question]) -> String {
    let state = match state {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    let mut out = format!("STATE:\n{state}\n\nQUESTIONS:\n");
    for question in questions {
        match &question.kind {
            Kind::Bool => {
                out.push_str(&format!(
                    "- {} (yes/no): {}\n",
                    question.id, question.instructions
                ));
            }
            Kind::Choice(options) => {
                out.push_str(&format!(
                    "- {} (one key): {}\n",
                    question.id, question.instructions
                ));
                for (key, meaning) in options {
                    out.push_str(&format!("    {key}: {meaning}\n"));
                }
            }
            Kind::Score(levels) => {
                out.push_str(&format!(
                    "- {} (level 0 to {}): {}\n",
                    question.id,
                    levels.len().saturating_sub(1),
                    question.instructions
                ));
                for (index, level) in levels.iter().enumerate() {
                    out.push_str(&format!("    {index}: {level}\n"));
                }
            }
        }
    }
    out
}

/// Read a chat model's reply. The outermost object is taken, so a stray
/// sentence or a code fence around it does not lose the answers; one that
/// does not fit its question is left out.
pub fn parse(reply: &str, questions: &[Question]) -> Result<BTreeMap<String, Answer>> {
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else {
        bail!("the reply holds no JSON object");
    };
    if end < start {
        bail!("the reply holds no JSON object");
    }
    let value: Value =
        serde_json::from_str(&reply[start..=end]).context("the reply is not valid JSON")?;
    let mut out = BTreeMap::new();
    for question in questions {
        let Some(raw) = value.get(&question.id) else {
            continue;
        };
        let confidence = raw
            .get("confidence")
            .and_then(Value::as_f64)
            .map(|c| c.clamp(0.0, 1.0) as f32)
            .unwrap_or(0.5);
        let answer = raw.get("answer").unwrap_or(raw);
        let parsed = match &question.kind {
            Kind::Bool => answer.as_bool().map(|yes| Answer::Bool {
                yes: if yes { confidence } else { 1.0 - confidence },
            }),
            Kind::Choice(options) => answer
                .as_str()
                .filter(|key| options.iter().any(|(k, _)| k == key))
                .map(|key| Answer::Choice {
                    key: key.to_owned(),
                    probabilities: BTreeMap::from([(key.to_owned(), confidence)]),
                }),
            Kind::Score(levels) => answer
                .as_f64()
                .filter(|n| *n >= 0.0 && *n <= levels.len().saturating_sub(1) as f64)
                .map(|value| Answer::Score {
                    value: value as f32,
                    confidence,
                }),
        };
        if let Some(parsed) = parsed {
            out.insert(question.id.clone(), parsed);
        }
    }
    Ok(out)
}

#[async_trait]
impl DecisionProvider for LlmJudge {
    fn id(&self) -> &str {
        "llm"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn decide(&self, _name: &str, state: &Value, questions: &[Question]) -> Result<Decision> {
        let started = Instant::now();
        let messages = [
            Message::system(SYSTEM),
            Message::user(prompt(state, questions)),
        ];
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let completion = self.provider.complete(&messages, &[], &tx).await?;
        Ok(Decision {
            answers: parse(&completion.text, questions)?,
            latency_ms: started.elapsed().as_millis() as u64,
            provider: "llm".into(),
            model: self.model.clone(),
            fallback_used: false,
        })
    }
}
