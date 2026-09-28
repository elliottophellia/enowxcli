//! Run the interface suite on the configured model and score what it builds,
//! to compare a change to the interface agents' prompts or skills with the
//! run before it.
//!
//! cargo run -q -p enowx-core --example fe_eval -- [--case NAME]... [--agent fe]
//!     [--model ID] [--jobs N] [--own-setup] [--compare FILE] [--list] [--seeds]
//!
//! `--model` runs the agent on that model for this run only, the way
//! `agent.models` would: run the suite once per candidate and compare.
//!
//! `--seeds` scores the files the seeded cases start with, without calling
//! the model: a check that the scoring (and the browser) works.
//!
//! This calls the configured provider, and each case costs what its run
//! costs. Each case runs in a fresh folder with the built-in agents and
//! skills only (`--own-setup` adds this machine's skills, instructions and
//! agent files). Questions are answered with their first option. The scores
//! are written as JSON to target/fe-eval/; `--compare` shows each case's
//! change from an earlier file. The preview part of the score needs Chrome,
//! Chromium, Edge or Brave.

use std::path::PathBuf;

use enowx_core::config::Config;
use enowx_core::eval::{self, Score, Setup};
use futures::StreamExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut wanted = Vec::new();
    let mut agent = "fe".to_owned();
    let mut jobs = 1usize;
    let mut setup = Setup::BuiltIn;
    let mut compare = None;
    let mut seeds_only = false;
    let mut model = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--case" => wanted.push(args.next().expect("--case NAME")),
            "--agent" => agent = args.next().expect("--agent NAME"),
            "--jobs" => jobs = args.next().and_then(|n| n.parse().ok()).expect("--jobs N"),
            "--own-setup" => setup = Setup::Own,
            "--model" => model = Some(args.next().expect("--model ID")),
            "--compare" => compare = Some(PathBuf::from(args.next().expect("--compare FILE"))),
            "--seeds" => seeds_only = true,
            "--list" => {
                for case in eval::cases() {
                    println!("{:<18} {}", case.name, case.prompt);
                }
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other}; see the top of fe_eval.rs"),
        }
    }
    let cases: Vec<_> = eval::cases()
        .into_iter()
        .filter(|case| wanted.is_empty() || wanted.iter().any(|w| w == case.name))
        .collect();
    anyhow::ensure!(
        !cases.is_empty(),
        "no case is named {wanted:?}; --list shows them"
    );
    let previous: Vec<Score> = match &compare {
        Some(path) => {
            let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            serde_json::from_value(saved["scores"].clone())?
        }
        None => Vec::new(),
    };

    if seeds_only {
        let mut scores = Vec::new();
        for case in cases.iter().filter(|case| !case.seed.is_empty()) {
            let dir = std::env::temp_dir().join(format!("enx-eval-seed-{}", case.name));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir)?;
            for (path, content) in case.seed {
                std::fs::write(dir.join(path), content)?;
            }
            // The seeded page, or the first page when the case adds one.
            let page = case
                .seed
                .iter()
                .map(|(path, _)| *path)
                .find(|path| *path == case.page || path.ends_with(".html"))
                .unwrap_or(case.page);
            scores.push(eval::measure(&dir, page, case.name).await);
        }
        println!("{}", eval::table(&scores, &[]));
        return Ok(());
    }

    let mut config = Config::load()?;
    if let Some(model) = model {
        config.agent.models.insert(agent.clone(), model);
    }
    let tier = enowx_core::builtin_agents()
        .into_iter()
        .find(|def| def.name == agent)
        .map_or(enowx_core::Tier::Balanced, |def| def.tier);
    let model = config.model_for(&agent, tier);
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let root = std::env::temp_dir().join(format!("enx-eval-{stamp}"));
    std::fs::create_dir_all(&root)?;
    println!("model  {model}");
    println!("agent  {agent}");
    println!("folder {}", root.display());
    if enowx_core::preview::find_chrome().is_none() {
        println!("no browser found: the scores leave the preview out");
    }

    let scores: Vec<Score> = futures::stream::iter(cases.iter())
        .map(|case| {
            let (config, agent, root) = (&config, &agent, &root);
            async move {
                println!("  {} started", case.name);
                let score = eval::run_case(config, agent, case, root, setup).await;
                println!(
                    "  {} finished: {} points in {}s",
                    case.name, score.points, score.seconds
                );
                score
            }
        })
        .buffered(jobs.max(1))
        .collect()
        .await;

    println!("\n{}", eval::table(&scores, &previous));
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/fe-eval");
    std::fs::create_dir_all(&out_dir)?;
    let out = out_dir.join(format!("{stamp}.json"));
    let saved = serde_json::json!({
        "when": stamp,
        "model": model,
        "agent": agent,
        "setup": if setup == Setup::Own { "own" } else { "built-in" },
        "scores": scores,
    });
    std::fs::write(&out, serde_json::to_string_pretty(&saved)?)?;
    println!("saved {}", std::fs::canonicalize(&out)?.display());
    println!("compare the next run with --compare {}", out.display());
    Ok(())
}
