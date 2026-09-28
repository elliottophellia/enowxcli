//! A fixed set of interface tasks, run on the configured model and scored, so
//! a change to the interface agents' prompts or skills is measured rather
//! than guessed at.
//!
//! Each case starts in a fresh workspace, some with files already in it, and
//! runs one request through an agent, answering every question with its
//! first option (the one agents are told to recommend). What is left is
//! scored with `ui_check` on the interface files and `preview` on the page,
//! beside what the run cost. Scores compare runs of the same suite; they are
//! not a grade of taste.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::{preview, ui_check, Agent, Discovery, Event, Role, SessionStore};

/// One task of the suite.
pub struct Case {
    pub name: &'static str,
    pub prompt: &'static str,
    /// Files the workspace starts with.
    pub seed: &'static [(&'static str, &'static str)],
    /// The page looked at afterwards.
    pub page: &'static str,
}

/// The suite: pages from nothing, a generated page to fix, and a page added
/// to an application that already has a direction.
pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "bakery",
            prompt: "Build a one-page site in plain HTML and CSS for a neighbourhood \
                     bakery that takes pre-orders for weekend bread. Put it in index.html \
                     at the root.",
            seed: &[],
            page: "index.html",
        },
        Case {
            name: "support-dashboard",
            prompt: "Build a dashboard in plain HTML, CSS and JavaScript for a support team \
                     of five to see the open tickets, who has each one, and which have \
                     waited longest. Use sample data from a JavaScript file, shown as \
                     sample data. Put it in index.html at the root.",
            seed: &[],
            page: "index.html",
        },
        Case {
            name: "portfolio",
            prompt: "Build a portfolio site in plain HTML and CSS for a freelance wedding \
                     photographer. Put it in index.html at the root.",
            seed: &[],
            page: "index.html",
        },
        Case {
            name: "fix-generated",
            prompt: "This page looks generated. Make it look designed and fix its problems. \
                     It is for a bicycle repair shop in Bandung; keep it one page.",
            seed: &[("index.html", SLOP_HTML), ("styles.css", SLOP_CSS)],
            page: "index.html",
        },
        Case {
            name: "settings-page",
            prompt: "Add the settings page this app links to (settings.html): the clinic's \
                     details, the front desk's notification preferences, and signing out. \
                     Keep to the app's design.",
            seed: &[
                ("DESIGN.md", CLINIC_DESIGN),
                ("index.html", CLINIC_HTML),
                ("styles.css", CLINIC_CSS),
            ],
            page: "settings.html",
        },
    ]
}

/// What a case left behind, and what it cost. Fields missing from an older
/// saved run read as their defaults, so `--compare` still loads it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Score {
    pub case: String,
    pub points: i64,
    /// The seed's points, for a case that starts with files.
    pub before: Option<i64>,
    pub ui_high: usize,
    pub ui_medium: usize,
    pub ui_low: usize,
    /// `ui_check` rules that fired, with how often.
    pub rules: BTreeMap<String, usize>,
    /// The page previewed, relative to the workspace.
    pub page: String,
    pub page_missing: bool,
    /// Why there was no preview, other than a missing page.
    pub preview_skipped: Option<String>,
    /// Widths the page is wider than.
    pub overflow: Vec<u32>,
    pub low_contrast: usize,
    pub dead_links: usize,
    pub missing_alt: usize,
    pub unnamed_controls: usize,
    pub small_targets: usize,
    pub console_errors: usize,
    pub h1_count: u64,
    pub viewport_meta: bool,
    /// At some width, a long page's top bar scrolled away.
    pub header_scrolls_away: bool,
    /// At some width, a page of more than three screens had no way back to
    /// the top from halfway down.
    pub no_back_to_top: bool,
    pub design_md: bool,
    pub skills_read: Vec<String>,
    pub tools: BTreeMap<String, usize>,
    /// Whether the harness's `ui_check` sent the agent back.
    pub gate_fired: bool,
    pub steps: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: f64,
    pub seconds: u64,
    pub workspace: PathBuf,
    pub error: Option<String>,
}

/// Points out of 100: each finding costs what it would cost a person using
/// the page. The same finding at several widths counts once, except
/// overflow, which is a different fault at each width.
pub fn points(score: &Score) -> i64 {
    let mut lost = 8 * score.ui_high + 3 * score.ui_medium + score.ui_low;
    lost += 6 * score.overflow.len()
        + 2 * score.low_contrast
        + 4 * score.dead_links
        + 3 * score.missing_alt
        + 3 * score.unnamed_controls
        + score.small_targets
        + 4 * score.console_errors
        + 3 * usize::from(score.header_scrolls_away)
        + 3 * usize::from(score.no_back_to_top);
    if score.preview_skipped.is_none() && !score.page_missing {
        lost += 3 * usize::from(score.h1_count != 1) + 5 * usize::from(!score.viewport_meta);
    }
    lost += 40 * usize::from(score.page_missing) + 5 * usize::from(!score.design_md);
    (100 - lost as i64).max(0)
}

/// Score what is in `workspace` now: `ui_check`, then `preview` of `page`.
pub async fn measure(workspace: &Path, page: &str, case: &str) -> Score {
    let mut score = Score {
        case: case.to_owned(),
        workspace: workspace.to_path_buf(),
        design_md: workspace.join("DESIGN.md").is_file(),
        ..Score::default()
    };
    let files = ui_check::interface_files(workspace);
    for finding in ui_check::check(workspace, &files) {
        match finding.severity {
            ui_check::Severity::High => score.ui_high += 1,
            ui_check::Severity::Medium => score.ui_medium += 1,
            ui_check::Severity::Low => score.ui_low += 1,
        }
        *score.rules.entry(finding.rule.to_owned()).or_default() += 1;
    }

    let Some(page) = find_page(workspace, page) else {
        score.page = page.to_owned();
        score.page_missing = true;
        score.points = points(&score);
        return score;
    };
    score.page = page
        .strip_prefix(workspace)
        .unwrap_or(&page)
        .display()
        .to_string();
    if preview::find_chrome().is_none() {
        score.preview_skipped = Some("no browser".into());
        score.points = points(&score);
        return score;
    }
    let shots = workspace.with_file_name(format!(
        "{}-shots",
        workspace.file_name().unwrap_or_default().to_string_lossy()
    ));
    match preview::preview(workspace, preview::Target::File(page), None, &shots).await {
        Ok(reports) => {
            let distinct = |pick: fn(&preview::WidthReport) -> &Vec<String>| {
                let mut all: Vec<&String> = reports.iter().flat_map(pick).collect();
                all.sort();
                all.dedup();
                all.len()
            };
            score.overflow = reports
                .iter()
                .filter(|r| r.overflow_px > 1)
                .map(|r| r.width)
                .collect();
            score.low_contrast = distinct(|r| &r.low_contrast);
            score.dead_links = distinct(|r| &r.dead_anchors);
            score.missing_alt = distinct(|r| &r.missing_alt);
            score.unnamed_controls = distinct(|r| &r.unnamed_controls);
            score.small_targets = distinct(|r| &r.small_targets);
            score.console_errors = distinct(|r| &r.console_errors);
            score.h1_count = reports.first().map_or(0, |r| r.h1_count);
            score.viewport_meta = reports.first().is_some_and(|r| r.viewport_meta);
            score.header_scrolls_away = reports.iter().any(|r| r.header_scrolls_away);
            score.no_back_to_top = reports.iter().any(|r| r.no_back_to_top);
        }
        Err(error) => score.preview_skipped = Some(format!("{error:#}")),
    }
    score.points = points(&score);
    score
}

/// `page`, or a page of the same name the agent put in a folder.
fn find_page(workspace: &Path, page: &str) -> Option<PathBuf> {
    let wanted = workspace.join(page);
    if wanted.is_file() {
        return Some(wanted);
    }
    let name = Path::new(page).file_name();
    ui_check::interface_files(workspace)
        .into_iter()
        .find(|p| p.file_name() == name)
}

/// What the agents run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    /// The built-in agents and skills only, so a run measures what enx
    /// ships and not what this machine adds to it.
    BuiltIn,
    /// Everything found on this machine too: your skills, instructions and
    /// agent files (MCP servers are never started).
    Own,
}

/// Run `case` through `agent` (its name, such as "fe") in a new workspace
/// under `root`, then score it.
pub async fn run_case(
    config: &Config,
    agent: &str,
    case: &Case,
    root: &Path,
    setup: Setup,
) -> Score {
    let workspace = root.join(case.name);
    let prepared = (|| -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&workspace)?;
        for (path, content) in case.seed {
            let path = workspace.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, content)?;
        }
        std::fs::canonicalize(&workspace)
    })();
    let workspace = match prepared {
        Ok(workspace) => workspace,
        Err(error) => {
            return Score {
                case: case.name.into(),
                error: Some(error.to_string()),
                workspace,
                ..Score::default()
            }
        }
    };
    // Only a page that exists before the run has a score to improve on.
    let before = if case.seed.iter().any(|(path, _)| *path == case.page) {
        Some(measure(&workspace, case.page, case.name).await.points)
    } else {
        None
    };

    let mut config = config.clone();
    config.agent.workspace = Some(workspace.clone());
    let discovery = match setup {
        Setup::BuiltIn => Discovery {
            agents: crate::builtin_agents(),
            skills: crate::discovery::skills::builtin_entries(),
            ..Discovery::default()
        },
        Setup::Own => {
            let mut discovery = Discovery::run(&workspace);
            discovery.mcp_servers.clear();
            discovery
        }
    };
    // Beside the workspace, so the agent never finds its own session files.
    let store = SessionStore::new(root.join(format!("{}-sessions", case.name)));
    let runner =
        Arc::new(Agent::with_discovery(config.clone(), store.clone(), discovery).asking_user());
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let request = crate::agent::RunRequest {
        prompt: case.prompt.to_owned(),
        session_id: None,
        role: Role::Orchestrator,
        attachments: Vec::new(),
        agent: Some(agent.to_owned()),
    };
    let started = Instant::now();
    let running = runner.clone();
    let cancel = tokio_util::sync::CancellationToken::new();
    let handle = tokio::spawn(async move { running.run(request, tx, cancel).await });

    let mut session = None;
    let mut tools: BTreeMap<String, usize> = BTreeMap::new();
    let mut skills_read = Vec::new();
    let mut gate_fired = false;
    let mut error = None;
    while let Some(event) = rx.recv().await {
        match event {
            Event::Session { id, .. } => {
                session.get_or_insert(id);
            }
            Event::ToolCall {
                name, arguments, ..
            } => {
                if name == "skill_read" {
                    if let Some(skill) = serde_json::from_str::<serde_json::Value>(&arguments)
                        .ok()
                        .and_then(|args| args["name"].as_str().map(str::to_owned))
                    {
                        skills_read.push(skill);
                    }
                }
                *tools.entry(name).or_default() += 1;
            }
            Event::Question { id, questions, .. } => {
                let replies = questions
                    .iter()
                    .map(|question| crate::ask::Reply {
                        chosen: question
                            .options
                            .iter()
                            .take(1)
                            .map(|o| o.label.clone())
                            .collect(),
                        other: String::new(),
                        notes: Vec::new(),
                    })
                    .collect();
                runner.answer(&id, crate::ask::Answer { replies });
            }
            Event::Notice { message } if message.starts_with("ui_check on the files changed") => {
                gate_fired = true;
            }
            Event::Error { message } => {
                error.get_or_insert(message);
            }
            _ => {}
        }
    }
    match handle.await {
        Ok(Err(failure)) => {
            error.get_or_insert(format!("{failure:#}"));
        }
        Err(panic) => {
            error.get_or_insert(panic.to_string());
        }
        Ok(Ok(_)) => {}
    }
    let seconds = started.elapsed().as_secs();

    let mut score = measure(&workspace, case.page, case.name).await;
    score.before = before;
    score.skills_read = skills_read;
    score.tools = tools;
    score.gate_fired = gate_fired;
    score.seconds = seconds;
    score.error = error;
    if let Some(saved) = session.and_then(|id| store.load(&id).ok()) {
        score.steps = saved
            .turns
            .iter()
            .filter(|turn| matches!(turn.message.role, crate::message::Role::Assistant))
            .count();
        score.input_tokens = u64::from(saved.usage.input_tokens);
        score.output_tokens = u64::from(saved.usage.output_tokens);
        score.cost = score.input_tokens as f64 * config.model.price_input / 1e6
            + score.output_tokens as f64 * config.model.price_output / 1e6;
    }
    score
}

/// The scores as a table, with the change from `previous` where it has the
/// same case.
pub fn table(scores: &[Score], previous: &[Score]) -> String {
    let mut out = format!(
        "{:<18} {:>6} {:>8}  {:<9} {:<6} {:>5} {:>8} {:>6}  {}\n",
        "case", "points", "change", "ui H/M/L", "design", "steps", "cost", "time", "preview"
    );
    for score in scores {
        let change = previous
            .iter()
            .find(|p| p.case == score.case)
            .map(|p| format!("{:+}", score.points - p.points))
            .or_else(|| score.before.map(|b| format!("from {b}")))
            .unwrap_or_default();
        let preview = if score.page_missing {
            format!("no {}", score.page)
        } else if let Some(skipped) = &score.preview_skipped {
            skipped.lines().next().unwrap_or("").to_owned()
        } else {
            let mut parts = Vec::new();
            if !score.overflow.is_empty() {
                parts.push(format!(
                    "wide@{}",
                    score
                        .overflow
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join("/")
                ));
            }
            for (n, label) in [
                (score.low_contrast, "contrast"),
                (score.dead_links, "dead"),
                (score.missing_alt, "alt"),
                (score.unnamed_controls, "unnamed"),
                (score.small_targets, "small"),
                (score.console_errors, "errors"),
            ] {
                if n > 0 {
                    parts.push(format!("{n} {label}"));
                }
            }
            if score.header_scrolls_away {
                parts.push("bar scrolls away".to_owned());
            }
            if score.no_back_to_top {
                parts.push("no back-to-top".to_owned());
            }
            if parts.is_empty() {
                "clean".to_owned()
            } else {
                parts.join(", ")
            }
        };
        out.push_str(&format!(
            "{:<18} {:>6} {:>8}  {:<9} {:<6} {:>5} {:>8} {:>5}s  {}\n",
            score.case,
            score.points,
            change,
            format!("{}/{}/{}", score.ui_high, score.ui_medium, score.ui_low),
            if score.design_md { "yes" } else { "no" },
            score.steps,
            format!("${:.3}", score.cost),
            score.seconds,
            preview,
        ));
    }
    let total: i64 = scores.iter().map(|s| s.points).sum();
    out.push_str(&format!(
        "\nmean {:.1} over {} cases",
        total as f64 / scores.len().max(1) as f64,
        scores.len()
    ));
    for score in scores {
        let mut notes = Vec::new();
        if let Some(error) = &score.error {
            notes.push(format!("error: {}", error.lines().next().unwrap_or("")));
        }
        if !score.rules.is_empty() {
            notes.push(
                score
                    .rules
                    .iter()
                    .map(|(rule, n)| format!("{rule}×{n}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
        if !score.skills_read.is_empty() {
            notes.push(format!("read {}", score.skills_read.join(", ")));
        }
        if score.gate_fired {
            notes.push("sent back by the ui_check gate".into());
        }
        if !notes.is_empty() {
            out.push_str(&format!("\n\n{}\n  {}", score.case, notes.join("\n  ")));
        }
    }
    out.push('\n');
    out
}

const SLOP_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>BikeFix Pro — Revolutionary Bike Repair</title>
<link rel="stylesheet" href="styles.css">
</head>
<body>
<nav class="nav glass">
  <a href="#" class="logo">🚲 BikeFix Pro</a>
  <a href="#">Features</a>
  <a href="#">Pricing</a>
  <a href="#" class="btn">Get Started</a>
</nav>
<section class="hero">
  <h1 class="gradient-text">Unlock Seamless Cycling — Powered by Passion</h1>
  <p>Revolutionary bike repair that elevates your riding journey to the next level.</p>
  <a href="#" class="btn glow">Get Started</a>
  <a href="#" class="btn ghost">Learn More</a>
</section>
<section class="stats">
  <div><strong>10,000+</strong><span>Happy riders</span></div>
  <div><strong>99.9%</strong><span>Satisfaction</span></div>
  <div><strong>24/7</strong><span>Support</span></div>
</section>
<section class="features">
  <div class="card glass"><div class="icon">⚡</div><h3>Lightning Fast</h3><p>Seamless repairs in record time.</p></div>
  <div class="card glass"><div class="icon">✨</div><h3>Premium Quality</h3><p>Cutting-edge tools for every bike.</p></div>
  <div class="card glass"><div class="icon">🚀</div><h3>Next-Level Service</h3><p>We empower riders everywhere.</p></div>
</section>
<section class="testimonial">
  <p>"BikeFix Pro changed my life!" — Sarah Johnson, CEO</p>
</section>
<footer><p>© 2026 BikeFix Pro. All rights reserved.</p></footer>
<button class="fab" onclick=""><svg width="20" height="20"><circle cx="10" cy="10" r="8"/></svg></button>
</body>
</html>
"##;

const SLOP_CSS: &str = "* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: 'Inter', sans-serif; background: #0b0b1a; color: #e5e5ff; }
.nav { display: flex; gap: 24px; padding: 16px 32px; position: sticky; top: 0; }
.glass { background: rgba(255,255,255,0.08); backdrop-filter: blur(12px); border: 1px solid rgba(255,255,255,0.15); }
.hero { min-height: 100vh; display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; background: linear-gradient(135deg, #6366f1, #a855f7, #ec4899); }
.gradient-text { background: linear-gradient(90deg, #60a5fa, #a78bfa); -webkit-background-clip: text; color: transparent; font-size: 64px; }
.hero p { color: #9ca3af; }
.btn { padding: 8px 14px; border-radius: 999px; background: #6366f1; color: #fff; }
.glow { box-shadow: 0 0 40px rgba(168,85,247,0.8); }
.stats { display: flex; justify-content: space-around; padding: 80px; }
.features { display: grid; grid-template-columns: repeat(3, 1fr); gap: 32px; padding: 80px; width: 1200px; }
.card { padding: 32px; border-radius: 24px; box-shadow: 0 20px 60px rgba(0,0,0,0.5); text-align: center; }
.icon { font-size: 40px; }
.testimonial { padding: 80px; text-align: center; font-style: italic; }
footer { padding: 40px; text-align: center; color: #555; }
.fab { position: fixed; bottom: 24px; right: 24px; border-radius: 50%; }
";

const CLINIC_DESIGN: &str = "# Design

Direction: a quiet tool for a small clinic's front desk; plain and dense,
nothing decorative.

Theme: light only.

Type: the system UI stack; 15px body, 13px secondary; headings 20px and
16px at weight 600.

Colour: tokens in styles.css. Ink #1f2328, muted #59636e, surface #ffffff,
canvas #f6f8fa, line #d1d9e0, accent #0969da (links and the primary action
only), danger #cf222e.

Shape: 6px radius; 1px lines, no shadows except menus.

Icons: none so far; if needed, Tabler outline at 18px.
";

const CLINIC_HTML: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Appointments · Front desk</title>
<link rel="stylesheet" href="styles.css">
</head>
<body>
<div class="app">
  <nav class="side" aria-label="Main">
    <a href="index.html" aria-current="page">Appointments</a>
    <a href="settings.html">Settings</a>
  </nav>
  <main>
    <h1>Appointments</h1>
    <table>
      <thead><tr><th>Time</th><th>Patient</th><th>With</th></tr></thead>
      <tbody>
        <tr><td>[Time]</td><td>[Patient name]</td><td>[Practitioner]</td></tr>
      </tbody>
    </table>
  </main>
</div>
</body>
</html>
"##;

const CLINIC_CSS: &str = ":root {
  --ink: #1f2328;
  --muted: #59636e;
  --surface: #ffffff;
  --canvas: #f6f8fa;
  --line: #d1d9e0;
  --accent: #0969da;
  --danger: #cf222e;
  --radius: 6px;
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-6: 24px;
}
* { box-sizing: border-box; }
body { margin: 0; font: 15px/1.5 system-ui, -apple-system, \"Segoe UI\", sans-serif; color: var(--ink); background: var(--canvas); }
.app { display: grid; grid-template-columns: 220px 1fr; min-height: 100vh; }
.side { background: var(--surface); border-right: 1px solid var(--line); padding: var(--space-4); }
.side a { display: block; padding: var(--space-3); color: var(--ink); text-decoration: none; border-radius: var(--radius); }
.side a[aria-current=\"page\"] { background: var(--canvas); font-weight: 600; }
main { padding: var(--space-6); }
h1 { font-size: 20px; margin: 0 0 var(--space-4); }
table { width: 100%; border-collapse: collapse; background: var(--surface); border: 1px solid var(--line); }
th, td { text-align: left; padding: var(--space-2) var(--space-3); border-bottom: 1px solid var(--line); }
th { font-size: 13px; color: var(--muted); font-weight: 600; }
@media (max-width: 720px) {
  .app { grid-template-columns: 1fr; }
  .side { border-right: 0; border-bottom: 1px solid var(--line); }
}
";
