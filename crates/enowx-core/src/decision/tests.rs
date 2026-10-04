//! Every use against a fake provider: sure, unsure, late and failing, and
//! shadow mode. Nothing here reaches the network.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use async_trait::async_trait;
use serde_json::{json, Value};

use super::uses::{self, AskVerdict, Intent, Keep, ShellVerdict};
use super::*;

/// How the fake answers.
#[derive(Clone)]
enum Behave {
    /// These answers, at once.
    Answer(BTreeMap<String, Answer>),
    /// Never in time.
    Late,
    Fail,
}

struct Fake(Behave);

#[async_trait]
impl DecisionProvider for Fake {
    fn id(&self) -> &str {
        "fake"
    }

    fn model(&self) -> &str {
        "fake-1"
    }

    async fn decide(&self, _: &str, _: &Value, _: &[Question]) -> Result<Decision> {
        match &self.0 {
            Behave::Answer(answers) => Ok(Decision {
                answers: answers.clone(),
                ..Decision::default()
            }),
            Behave::Late => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                bail!("too late to matter")
            }
            Behave::Fail => bail!("the endpoint answered 500"),
        }
    }
}

fn yes(p: f32) -> Answer {
    Answer::Bool { yes: p }
}

fn pick(key: &str, p: f32) -> Answer {
    Answer::Choice {
        key: key.into(),
        probabilities: BTreeMap::from([(key.to_owned(), p)]),
    }
}

fn answers(pairs: &[(&str, Answer)]) -> Behave {
    Behave::Answer(
        pairs
            .iter()
            .map(|(id, a)| ((*id).to_owned(), a.clone()))
            .collect(),
    )
}

fn decider_with(behave: Behave, shadow: bool) -> Decider {
    let config = DecisionConfig {
        enabled: true,
        timeout_ms: 50,
        shadow,
        ..DecisionConfig::default()
    };
    Decider::new(config, Arc::new(Fake(behave)), DecisionLog::memory())
}

fn decider(behave: Behave) -> Decider {
    decider_with(behave, false)
}

fn outcomes(decider: &Decider) -> Vec<String> {
    decider
        .log()
        .recorded()
        .into_iter()
        .map(|r| r.outcome)
        .collect()
}

// ---------------------------------------------------------------- off

#[tokio::test]
async fn off_changes_nothing_and_records_nothing() {
    let off = Decider::off();
    assert_eq!(uses::intent(&off, "build a shop").await.verdict, None);
    assert_eq!(uses::routing(&off, "x", &agents()).await.verdict, None);
    assert_eq!(
        uses::ask(&off, "x", &json!({})).await.verdict,
        AskVerdict::Allow
    );
    let long = "x".repeat(5000);
    assert_eq!(
        uses::tool_result(&off, "t", "bash", "{}", &long, false)
            .await
            .verdict,
        Keep::Full
    );
    assert_eq!(
        uses::shell(&off, "rm -rf /").await.verdict,
        ShellVerdict::Run
    );
    assert!(off.log().recorded().is_empty());
}

#[test]
fn the_default_config_is_off() {
    let config = crate::config::Config::default();
    assert!(!config.decision.enabled);
    let decider = Decider::from_config(&config);
    assert!(Use::ALL.iter().all(|u| !decider.is_on(*u)));
}

#[test]
fn a_switched_off_use_is_not_consulted() {
    let mut config = DecisionConfig {
        enabled: true,
        ..DecisionConfig::default()
    };
    config.uses.shell.enabled = false;
    let decider = Decider::new(config, Arc::new(Fake(answers(&[]))), DecisionLog::memory());
    assert!(!decider.is_on(Use::Shell));
    assert!(decider.is_on(Use::Intent));
}

// ---------------------------------------------------------------- intent

#[tokio::test]
async fn a_sure_build_answer_tells_the_lead_to_build() {
    let d = decider(answers(&[
        ("wants_brainstorm", yes(0.03)),
        ("scope", pick("small", 0.95)),
        (
            "ambiguity",
            Answer::Score {
                value: 0.4,
                confidence: 0.9,
            },
        ),
    ]));
    let ruling = uses::intent(&d, "make a countdown page").await;
    assert_eq!(ruling.verdict, Some(Intent::Build));
    assert_eq!(ruling.record.unwrap().outcome, "applied");
}

#[tokio::test]
async fn a_sure_brainstorm_answer_tells_the_lead_to_brainstorm() {
    let d = decider(answers(&[("wants_brainstorm", yes(0.96))]));
    assert_eq!(
        uses::intent(&d, "let's brainstorm a todo app")
            .await
            .verdict,
        Some(Intent::Brainstorm)
    );
}

#[tokio::test]
async fn an_unsure_intent_is_left_to_the_lead() {
    let d = decider(answers(&[("wants_brainstorm", yes(0.5))]));
    let ruling = uses::intent(&d, "a todo app?").await;
    assert_eq!(ruling.verdict, None);
    assert_eq!(ruling.record.unwrap().outcome, "unsure");
}

#[tokio::test]
async fn an_unreadable_product_is_left_to_the_lead() {
    let d = decider(answers(&[
        ("wants_brainstorm", yes(0.05)),
        ("scope", pick("product", 0.9)),
        (
            "ambiguity",
            Answer::Score {
                value: 3.4,
                confidence: 0.8,
            },
        ),
    ]));
    assert_eq!(
        uses::intent(&d, "build me something big").await.verdict,
        None
    );
}

#[tokio::test]
async fn a_late_or_failed_intent_takes_the_old_path() {
    for behave in [Behave::Late, Behave::Fail] {
        let d = decider(behave);
        let ruling = uses::intent(&d, "make a page").await;
        assert_eq!(ruling.verdict, None);
        let outcome = ruling.record.unwrap().outcome;
        assert!(outcome == "timeout" || outcome == "error", "{outcome}");
    }
}

#[tokio::test]
async fn shadow_records_the_decision_and_acts_on_nothing() {
    let d = decider_with(answers(&[("wants_brainstorm", yes(0.02))]), true);
    let ruling = uses::intent(&d, "make a page").await;
    assert_eq!(
        ruling.verdict, None,
        "shadow mode must not change behaviour"
    );
    assert_eq!(outcomes(&d), vec!["shadow"]);
}

// ---------------------------------------------------------------- routing

fn agents() -> Vec<(String, String)> {
    vec![
        ("fe".into(), "Frontend web".into()),
        ("be".into(), "Backend and APIs".into()),
        ("canvas".into(), "Static pages and visuals".into()),
    ]
}

#[tokio::test]
async fn a_sure_specialist_is_hinted() {
    let d = decider(answers(&[
        ("specialist", pick("canvas", 0.92)),
        ("needs_security_review", yes(0.02)),
    ]));
    let route = uses::routing(&d, "a countdown page", &agents())
        .await
        .verdict
        .expect("a route");
    assert_eq!(route.agent.as_deref(), Some("canvas"));
    assert_eq!(route.security, Some(false));
    assert!(uses::route_note(&route).contains("`canvas`"));
}

#[tokio::test]
async fn an_unsure_or_none_specialist_is_left_to_the_lead() {
    for answer in [pick("fe", 0.4), pick("none", 0.99)] {
        let d = decider(answers(&[
            ("specialist", answer),
            ("needs_security_review", yes(0.5)),
        ]));
        assert_eq!(uses::routing(&d, "x", &agents()).await.verdict, None);
    }
}

#[tokio::test]
async fn a_failed_routing_takes_the_old_path() {
    for behave in [Behave::Late, Behave::Fail] {
        let d = decider(behave);
        assert_eq!(uses::routing(&d, "x", &agents()).await.verdict, None);
    }
}

// ---------------------------------------------------------------- ask

#[tokio::test]
async fn a_question_that_is_not_the_users_is_refused() {
    let d = decider(answers(&[
        ("belongs_to_user", yes(0.04)),
        ("category", pick("derivable", 0.9)),
    ]));
    assert_eq!(
        uses::ask(&d, "a page", &json!({"q": "which framework?"}))
            .await
            .verdict,
        AskVerdict::Refuse
    );
}

#[tokio::test]
async fn a_question_that_is_the_users_is_asked() {
    let d = decider(answers(&[("belongs_to_user", yes(0.95))]));
    assert_eq!(
        uses::ask(&d, "a page", &json!({"q": "which brand colour?"}))
            .await
            .verdict,
        AskVerdict::Allow
    );
}

#[tokio::test]
async fn an_unsure_question_is_marked_unsure() {
    let d = decider(answers(&[("belongs_to_user", yes(0.55))]));
    assert_eq!(
        uses::ask(&d, "x", &json!({})).await.verdict,
        AskVerdict::Unsure
    );
}

#[tokio::test]
async fn a_failed_ask_judgement_lets_the_question_through() {
    for behave in [Behave::Late, Behave::Fail] {
        let d = decider(behave);
        assert_eq!(
            uses::ask(&d, "x", &json!({})).await.verdict,
            AskVerdict::Allow
        );
    }
}

#[tokio::test]
async fn shadow_never_refuses_a_question() {
    let d = decider_with(answers(&[("belongs_to_user", yes(0.01))]), true);
    assert_eq!(
        uses::ask(&d, "x", &json!({})).await.verdict,
        AskVerdict::Allow
    );
}

// ---------------------------------------------------------------- tool results

#[tokio::test]
async fn a_sure_noise_result_is_cut() {
    let d = decider(answers(&[
        ("relevance", pick("noise", 0.95)),
        ("keep", pick("excerpt", 0.93)),
    ]));
    let long = "line\n".repeat(2000);
    let ruling = uses::tool_result(&d, "t", "bash", "{}", &long, false).await;
    assert_eq!(ruling.verdict, Keep::Excerpt);
    let cut = uses::apply_keep(ruling.verdict, &long);
    assert!(cut.len() < long.len() / 5);
    assert!(cut.contains("left out of context"));
}

#[tokio::test]
async fn an_essential_result_is_never_cut() {
    let d = decider(answers(&[
        ("relevance", pick("essential", 0.9)),
        ("keep", pick("summary", 0.95)),
    ]));
    let long = "x".repeat(5000);
    assert_eq!(
        uses::tool_result(&d, "t", "read", "{}", &long, false)
            .await
            .verdict,
        Keep::Full
    );
}

#[tokio::test]
async fn an_unsure_failed_or_short_result_is_kept_whole() {
    let long = "x".repeat(5000);
    let unsure = decider(answers(&[("keep", pick("summary", 0.6))]));
    assert_eq!(
        uses::tool_result(&unsure, "t", "bash", "{}", &long, false)
            .await
            .verdict,
        Keep::Full
    );
    for behave in [Behave::Late, Behave::Fail] {
        let d = decider(behave);
        assert_eq!(
            uses::tool_result(&d, "t", "bash", "{}", &long, false)
                .await
                .verdict,
            Keep::Full
        );
    }
    let sure = decider(answers(&[("keep", pick("excerpt", 0.99))]));
    let short = uses::tool_result(&sure, "t", "bash", "{}", "ok", false).await;
    assert_eq!(short.verdict, Keep::Full);
    assert!(short.record.is_none(), "a short result is never sent");
    let error = uses::tool_result(&sure, "t", "bash", "{}", &long, true).await;
    assert_eq!(error.verdict, Keep::Full);
    assert!(error.record.is_none(), "an error is never sent");
}

#[test]
fn a_summary_keeps_the_opening_lines_and_says_what_went() {
    let content: String = (0..100).map(|i| format!("row {i}\n")).collect();
    let summary = uses::apply_keep(Keep::Summary, &content);
    assert!(summary.starts_with("row 0\n"));
    assert!(summary.contains("88 more lines"));
    assert!(!summary.contains("row 50"));
}

#[test]
fn cutting_respects_character_boundaries() {
    let content = "日本語テキスト".repeat(500);
    let cut = uses::apply_keep(Keep::Excerpt, &content);
    assert!(cut.starts_with('日'));
    assert!(cut.len() < content.len());
}

// ---------------------------------------------------------------- shell

#[tokio::test]
async fn a_sure_read_only_command_runs() {
    let d = decider(answers(&[("risk", pick("read_only", 0.97))]));
    assert_eq!(uses::shell(&d, "ls -la").await.verdict, ShellVerdict::Run);
}

#[tokio::test]
async fn a_sure_destructive_command_asks_first() {
    let d = decider(answers(&[("risk", pick("destructive", 0.95))]));
    assert!(matches!(
        uses::shell(&d, "rm -rf build").await.verdict,
        ShellVerdict::Confirm(_)
    ));
}

#[tokio::test]
async fn an_unsure_shell_answer_always_asks() {
    for answer in [pick("read_only", 0.6), pick("local_write", 0.5)] {
        let d = decider(answers(&[("risk", answer)]));
        assert!(
            matches!(
                uses::shell(&d, "./deploy.sh").await.verdict,
                ShellVerdict::Confirm(_)
            ),
            "a security gate never defaults to allow"
        );
    }
    let empty = decider(answers(&[]));
    assert!(matches!(
        uses::shell(&empty, "./x").await.verdict,
        ShellVerdict::Confirm(_)
    ));
}

#[tokio::test]
async fn a_late_or_failed_shell_judgement_runs_as_before() {
    for behave in [Behave::Late, Behave::Fail] {
        let d = decider(behave);
        assert_eq!(
            uses::shell(&d, "rm -rf build").await.verdict,
            ShellVerdict::Run
        );
    }
}

#[tokio::test]
async fn shadow_never_stops_a_command() {
    let d = decider_with(answers(&[("risk", pick("production", 0.99))]), true);
    assert_eq!(
        uses::shell(&d, "kubectl delete ns prod").await.verdict,
        ShellVerdict::Run
    );
    assert_eq!(outcomes(&d), vec!["shadow"]);
}

#[test]
fn the_shell_question_is_a_valid_ask() {
    let args = uses::shell_question("rm -rf build", "it looks destructive (0.95)");
    let questions = crate::ask::parse(&args).expect("ask accepts it");
    assert_eq!(questions[0].options.len(), 2);
}

// ---------------------------------------------------------------- wire

#[test]
fn jev_answers_are_read_by_type() {
    let questions = [
        Question::boolean("keep", "?"),
        Question::choice("who", "?", &[("fe", ""), ("be", "")]),
        Question::score("level", "?", &["a", "b", "c"]),
    ];
    let raw = json!({"answers": {
        "keep": {"type": "noul", "noul": 0.87},
        "who": {"type": "choice", "choice": "be", "probabilities": {"fe": 0.1, "be": 0.9}, "confidence": 0.8},
        "level": {"type": "score", "score": 1.25, "confidence": 0.65}
    }});
    let parsed = jev::parse(&raw, &questions);
    let d = Decision {
        answers: parsed,
        ..Decision::default()
    };
    assert_eq!(d.yes("keep"), Some(0.87));
    assert_eq!(d.choice("who"), Some(("be", 0.9)));
    assert_eq!(d.score("level"), Some(1.25));
    assert_eq!(d.yes("who"), None, "a choice is not a yes/no");
}

#[test]
fn workers_ai_answers_are_unwrapped_and_bad_choices_dropped() {
    let questions = [
        Question::boolean("ok", "?"),
        Question::choice("who", "?", &[("fe", "")]),
    ];
    let raw = json!({"success": true, "result": {"answers": {
        "ok": {"noul": 0.99},
        "who": {"choice": "not-an-option", "confidence": 0.9}
    }}});
    let parsed = jev::parse(&raw, &questions);
    assert!(parsed.contains_key("ok"));
    assert!(
        !parsed.contains_key("who"),
        "an answer outside the options is not used"
    );
}

#[test]
fn a_request_carries_typed_questions() {
    let jev = JevCompat::jev(JEV_URL, "k", "jev-latest");
    let body = jev.body(
        &json!("state"),
        &[Question::choice("who", "pick", &[("fe", "web")])],
    );
    assert_eq!(body["model"], "jev-latest");
    assert_eq!(body["questions"]["who"]["type"], "choice");
    assert_eq!(body["questions"]["who"]["criteria"]["fe"], "web");
    let clef = JevCompat::clef("acct", "k", "@cf/cloudflare/clef-flash");
    assert!(clef
        .url()
        .ends_with("/accounts/acct/ai/run/@cf/cloudflare/clef-flash"));
    assert!(clef.body(&json!(1), &[]).get("model").is_none());
}

#[test]
fn an_llm_reply_is_read_with_its_confidence() {
    let questions = [
        Question::boolean("ok", "?"),
        Question::choice("who", "?", &[("fe", ""), ("be", "")]),
        Question::score("level", "?", &["a", "b"]),
    ];
    let reply = "Here you go:\n```json\n{\"ok\": {\"answer\": false, \"confidence\": 0.9}, \
                 \"who\": {\"answer\": \"fe\", \"confidence\": 0.7}, \
                 \"level\": {\"answer\": 7, \"confidence\": 0.9}}\n```";
    let parsed = llm::parse(reply, &questions).expect("parses");
    let d = Decision {
        answers: parsed,
        ..Decision::default()
    };
    assert!(
        (d.yes("ok").unwrap() - 0.1).abs() < 1e-6,
        "no at 0.9 is yes at 0.1"
    );
    assert_eq!(d.choice("who"), Some(("fe", 0.7)));
    assert_eq!(d.score("level"), None, "a level past the scale is dropped");
    assert!(llm::parse("no json here", &questions).is_err());
}

// ---------------------------------------------------------------- config

#[test]
fn a_provider_without_what_it_needs_is_not_built() {
    let mut config = crate::config::Config::default();
    config.decision.enabled = true;
    config.decision.provider = "clef".into();
    config.decision.account_id = "acct".into();
    std::env::remove_var("CLOUDFLARE_API_TOKEN");
    let error = build_provider(&config)
        .err()
        .expect("no token, no provider");
    assert!(error.contains("token"), "{error}");
    config.decision.provider = "custom".into();
    config.decision.base_url = "not a url".into();
    assert!(build_provider(&config).is_err());
    config.decision.base_url = "http://127.0.0.1:9/decide".into();
    assert!(
        build_provider(&config).is_ok(),
        "a local endpoint may have no key"
    );
    config.decision.provider = "nope".into();
    assert!(build_provider(&config).is_err());
    let off = Decider::from_config(&config);
    assert!(
        !off.is_on(Use::Intent),
        "a provider that cannot be built is off"
    );
}

#[test]
fn the_decision_table_round_trips_through_toml() {
    let text = "[decision]\nenabled = true\nprovider = \"jev\"\nshadow = true\n\n\
                [decision.uses.shell]\nenabled = false\nthreshold = 0.95\n";
    let config: crate::config::Config = toml::from_str(text).expect("parses");
    assert!(config.decision.enabled && config.decision.shadow);
    assert_eq!(config.decision.provider, "jev");
    assert_eq!(config.decision.model(), "jev-latest");
    assert!(!config.decision.uses.shell.enabled);
    assert_eq!(config.decision.uses.shell.threshold, 0.95);
    assert_eq!(
        config.decision.uses.routing.threshold, 0.7,
        "untouched uses keep defaults"
    );
    assert_eq!(config.decision.timeout_ms, 300);
}
