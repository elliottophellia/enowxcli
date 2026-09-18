//! A thin client for TypeSafe's System One endpoint.
//!
//! This is used for small typed judgements inside the harness — "is this tool
//! result worth keeping?", "is this turn still live?" — not for answering the
//! user. It returns probabilities, never prose.
//!
//! Every call here is an optimisation. The caller must have a correct answer
//! without it, because the network can be slow, the key can be missing, and
//! the service can be down. Failure returns `None` and the caller carries on
//! with the rules it had before.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::TypeSafeConfig;

/// One yes/no judgement and the probability the answer is yes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noul {
    pub probability: f32,
}

/// A position on an ordered set of levels, plus how concentrated the
/// distribution behind it was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub value: f32,
    pub confidence: f32,
}

#[derive(Clone)]
pub struct SystemOne {
    client: reqwest::Client,
    config: TypeSafeConfig,
}

#[derive(Deserialize)]
struct Response {
    answers: std::collections::BTreeMap<String, Answer>,
}

#[derive(Deserialize)]
struct Answer {
    #[serde(default)]
    noul: Option<f32>,
    #[serde(default)]
    score: Option<f32>,
    #[serde(default)]
    confidence: Option<f32>,
}

impl SystemOne {
    /// `None` when no key is configured: the harness then never calls out at
    /// all, rather than building requests it will throw away.
    pub fn new(config: &TypeSafeConfig) -> Option<Self> {
        if !config.active() {
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(100)))
            .build()
            .ok()?;
        Some(Self {
            client,
            config: config.clone(),
        })
    }

    /// Ask several questions about one state in a single request. They are
    /// answered in parallel and cannot see one another, which is why batching
    /// is free: one round trip instead of several.
    ///
    /// Returns `None` on any failure. A judgement that did not arrive must
    /// look exactly like one that was never asked for.
    pub async fn ask(&self, state: Value, questions: Value) -> Option<Judgements> {
        let body = json!({
            "model": self.config.model,
            "state": state,
            "questions": questions,
        });
        let response = self
            .client
            .post(&self.config.base_url)
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let parsed: Response = response.json().await.ok()?;
        Some(Judgements {
            answers: parsed.answers,
        })
    }
}

/// The answers to one request, looked up by the ids the caller chose.
pub struct Judgements {
    answers: std::collections::BTreeMap<String, Answer>,
}

impl Judgements {
    pub fn noul(&self, id: &str) -> Option<Noul> {
        let answer = self.answers.get(id)?;
        Some(Noul {
            probability: answer.noul?,
        })
    }

    pub fn score(&self, id: &str) -> Option<Score> {
        let answer = self.answers.get(id)?;
        Some(Score {
            value: answer.score?,
            confidence: answer.confidence.unwrap_or(0.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_key_means_no_client() {
        let config = TypeSafeConfig::default();
        assert!(
            SystemOne::new(&config).is_none(),
            "a default install must not reach the network"
        );
    }

    #[test]
    fn a_key_produces_a_client() {
        let config = TypeSafeConfig {
            api_key: "test-key".into(),
            ..TypeSafeConfig::default()
        };
        assert!(SystemOne::new(&config).is_some());
    }

    /// Whitespace is not a key. Someone who sets the variable to nothing
    /// means "off", not "send an empty bearer token".
    #[test]
    fn a_blank_key_is_not_a_key() {
        let config = TypeSafeConfig {
            api_key: "   ".into(),
            ..TypeSafeConfig::default()
        };
        assert!(!config.active());
        assert!(SystemOne::new(&config).is_none());
    }

    #[test]
    fn answers_are_read_by_id() {
        let raw = r#"{"answers":{"keep":{"type":"noul","noul":0.87},
                      "live":{"type":"score","score":1.25,"confidence":0.65}}}"#;
        let parsed: Response = serde_json::from_str(raw).expect("parses");
        let j = Judgements {
            answers: parsed.answers,
        };
        assert_eq!(j.noul("keep").unwrap().probability, 0.87);
        let score = j.score("live").unwrap();
        assert_eq!(score.value, 1.25);
        assert_eq!(score.confidence, 0.65);
        assert!(j.noul("missing").is_none(), "an absent id is not an error");
        assert!(
            j.noul("live").is_none(),
            "a score is not a noul; reading it as one must not invent a value"
        );
    }
}
