//! Other coding agents as the brain of an enowx agent, over the Agent Client
//! Protocol.
//!
//! Claude Code, Codex, Gemini CLI, or any program that speaks ACP, runs as a
//! child process and enowx is its client, the way an editor is. The agent
//! brings its model and the user's own login or subscription; enowx brings
//! its prompts, skills, delegation and questions, through a local MCP server
//! it hands the agent. Which enowx agents run on which engine is set per
//! agent (`[acp.agents]`), so the lead can be Claude Code while `fe` is
//! Codex and the rest stay on the configured model.
//!
//! Adapters are installed into `~/.enx/acp` (never globally) or found on the
//! PATH. Nothing here sends the user's credentials anywhere: the agent reads
//! its own.

pub mod client;
pub mod manager;
pub mod mcp_host;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A built-in engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kind {
    pub id: &'static str,
    pub title: &'static str,
    /// The npm package that provides it.
    pub package: &'static str,
    /// The version enowx was tested against, and installs.
    pub version: &'static str,
    /// The executable the package installs.
    pub bin: &'static str,
    /// Arguments that start it in ACP mode.
    pub args: &'static [&'static str],
    /// The vendor CLI the user signs in with.
    pub cli: &'static str,
    pub login_hint: &'static str,
}

/// The engines enowx knows. Others are added as custom agents.
pub const KINDS: &[Kind] = &[
    Kind {
        id: "claude",
        title: "Claude Code",
        package: "@agentclientprotocol/claude-agent-acp",
        version: "0.85.1",
        bin: "claude-agent-acp",
        args: &[],
        cli: "claude",
        login_hint: "Run `claude` in a terminal and sign in with /login.",
    },
    Kind {
        id: "codex",
        title: "Codex",
        package: "@agentclientprotocol/codex-acp",
        version: "2.1.1",
        bin: "codex-acp",
        args: &[],
        cli: "codex",
        login_hint: "Run `codex login` in a terminal.",
    },
    Kind {
        id: "gemini",
        title: "Gemini CLI",
        package: "@google/gemini-cli",
        version: "0.62.0",
        bin: "gemini",
        args: &["--acp"],
        cli: "gemini",
        login_hint: "Run `gemini` in a terminal once and sign in.",
    },
];

pub fn kind(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

/// `[acp]`: which enowx agents run on which engine, and how each engine is
/// set up. Empty: every agent runs on the configured model, as before.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AcpConfig {
    /// The engine every agent runs on, set by `/acp <name>`; empty runs
    /// enowx's own (`/acp off`).
    pub active: String,
    /// Per-agent exceptions to `active`, by enowx agent name: an engine id
    /// (`claude`, `codex`, `gemini`, or a custom agent's name), or `enowx`
    /// for the configured model.
    pub agents: BTreeMap<String, String>,
    /// Per-engine settings, by engine id.
    pub engines: BTreeMap<String, EngineConfig>,
    /// Agents added by hand: any program that speaks ACP on stdio.
    pub custom: BTreeMap<String, CustomAgent>,
}

/// How an engine is run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EngineConfig {
    /// A model the engine offers; blank leaves its default.
    pub model: String,
    /// A thinking effort the engine offers; blank leaves its default.
    pub effort: String,
    /// `ask` (every permission the engine asks for goes to the user),
    /// `enowx` (enowx's own rules: allowed as enowx's tools are, a risky
    /// shell command through the decision model when it is on), or `bypass`
    /// (the engine's own bypass mode: nothing is asked; opt-in only).
    pub permission: String,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            effort: String::new(),
            permission: "ask".into(),
        }
    }
}

/// The permission modes, in the order Settings offers them.
pub const PERMISSIONS: [&str; 3] = ["ask", "enowx", "bypass"];

/// A permission mode in words.
pub fn permission_label(mode: &str) -> &'static str {
    match mode {
        "enowx" => "enowx's rules",
        "bypass" => "bypass",
        _ => "ask me",
    }
}

/// A program that speaks ACP on stdio, added by hand.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomAgent {
    /// The program: a path, or a name on the PATH.
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl AcpConfig {
    /// The engine `agent` runs on, when it is not the configured model.
    pub fn engine_for(&self, agent: &str) -> Option<&str> {
        let agent = crate::agent_def::canonical_name(agent);
        match self.agents.get(agent).map(String::as_str) {
            Some("enowx") => None,
            Some(id) if self.exists(id) => Some(id),
            _ => self.active().map(|_| self.active.as_str()),
        }
    }

    /// The engine `/acp` put the session on, when it still exists.
    pub fn active(&self) -> Option<&str> {
        let id = self.active.as_str();
        (!id.is_empty() && self.exists(id)).then_some(id)
    }

    /// Whether any agent runs on an engine.
    pub fn any_assigned(&self) -> bool {
        self.active().is_some() || self.agents.values().any(|id| self.exists(id))
    }

    /// What the status bar says about an engine: `Claude Code · opus · high ·
    /// ask me`.
    pub fn label(&self, id: &str) -> String {
        let engine = self.engine(id);
        let or_default = |s: &str| {
            if s.is_empty() {
                "default".to_owned()
            } else {
                s.to_owned()
            }
        };
        format!(
            "{} · {} · effort {} · {}",
            title(id),
            or_default(&engine.model),
            or_default(&engine.effort),
            permission_label(&engine.permission)
        )
    }

    /// Whether `id` names a built-in engine or a custom agent.
    pub fn exists(&self, id: &str) -> bool {
        kind(id).is_some() || self.custom.contains_key(id)
    }

    pub fn engine(&self, id: &str) -> EngineConfig {
        self.engines.get(id).cloned().unwrap_or_default()
    }

    /// Every engine, built-ins first: what an agent can be put on.
    pub fn engine_ids(&self) -> Vec<String> {
        KINDS
            .iter()
            .map(|k| k.id.to_owned())
            .chain(self.custom.keys().cloned())
            .collect()
    }
}

/// An engine's name on screen.
pub fn title(id: &str) -> String {
    kind(id).map_or_else(|| id.to_owned(), |k| k.title.to_owned())
}

/// Where enowx installs adapters: `~/.enx/acp`.
pub fn install_dir() -> PathBuf {
    crate::config::home_dir().join("acp")
}

/// The user's login-shell PATH, plus the usual places Node and the CLIs
/// live. A process started from a launcher inherits a bare PATH holding
/// neither, and the agent would fail for a reason the user cannot see.
pub fn login_path() -> &'static str {
    static PATH: OnceLock<String> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut parts: Vec<String> = Vec::new();
        if let Ok(own) = std::env::var("PATH") {
            parts.extend(std::env::split_paths(&own).map(|p| p.display().to_string()));
        }
        #[cfg(unix)]
        {
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
            if let Ok(out) = std::process::Command::new(shell)
                .args(["-lc", "printf %s \"$PATH\""])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
            {
                if out.status.success() {
                    parts.extend(
                        String::from_utf8_lossy(&out.stdout)
                            .split(':')
                            .filter(|s| !s.is_empty())
                            .map(String::from),
                    );
                }
            }
        }
        let home = dirs::home_dir().unwrap_or_default();
        for extra in [
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            home.join(".local/bin"),
            home.join(".volta/bin"),
            home.join(".bun/bin"),
        ] {
            parts.push(extra.display().to_string());
        }
        let mut seen = std::collections::HashSet::new();
        parts.retain(|p| seen.insert(p.clone()));
        std::env::join_paths(parts)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    })
}

/// An executable on the login PATH, or a path that is one.
pub fn which(name: &str) -> Option<PathBuf> {
    let direct = Path::new(name);
    if direct.components().count() > 1 {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    which::which_in(name, Some(login_path()), std::env::current_dir().ok()?).ok()
}

fn version_of(program: &Path) -> Option<String> {
    let out = std::process::Command::new(program)
        .arg("--version")
        .env("PATH", login_path())
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

/// The adapter enowx installed for `kind`, and its version.
fn installed_entry(kind: &Kind) -> Option<(PathBuf, String)> {
    let package = install_dir().join("node_modules").join(kind.package);
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(package.join("package.json")).ok()?).ok()?;
    let bin = manifest.get("bin").and_then(|b| match b {
        Value::String(path) => Some(path.as_str()),
        Value::Object(map) => map
            .get(kind.bin)
            .or_else(|| map.values().next())
            .and_then(Value::as_str),
        _ => None,
    })?;
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_owned();
    Some((package.join(bin), version))
}

/// What is there for one engine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Detection {
    pub id: String,
    pub title: String,
    pub node: Option<String>,
    /// `enowx` (installed into ~/.enx/acp), `path`, or `custom`.
    pub source: Option<&'static str>,
    pub adapter: Option<String>,
    pub adapter_version: Option<String>,
    /// The tested version, for a built-in engine.
    pub wanted_version: Option<&'static str>,
    pub cli: Option<String>,
    pub cli_version: Option<String>,
    pub login: Option<LoginStatus>,
}

impl Detection {
    /// Whether it can start now.
    pub fn ready(&self) -> bool {
        self.adapter.is_some() && (self.source != Some("enowx") || self.node.is_some())
    }

    /// One line for Settings.
    pub fn line(&self) -> String {
        if self.source == Some("custom") {
            return match &self.adapter {
                Some(path) => format!("ready · {path}"),
                None => "its command is not found".into(),
            };
        }
        let mut parts = Vec::new();
        match (&self.adapter, self.source) {
            (Some(_), Some("enowx")) => parts.push(format!(
                "adapter {} in ~/.enx/acp",
                self.adapter_version.as_deref().unwrap_or("?")
            )),
            (Some(_), _) => parts.push("adapter on PATH".into()),
            (None, _) => parts.push("not installed".into()),
        }
        if self.node.is_none() {
            parts.push("Node.js not found".into());
        }
        match &self.login {
            Some(login) if login.signed_in => parts.push(match &login.label {
                Some(label) => format!("signed in ({label})"),
                None => "signed in".into(),
            }),
            Some(_) => parts.push("not signed in".into()),
            None if self.cli.is_none() => {}
            None => {}
        }
        parts.join(" · ")
    }
}

/// What is there for every engine. Blocks: run it off the UI thread.
pub async fn detect(config: &AcpConfig) -> Vec<Detection> {
    let node = which("node").map(|p| p.display().to_string());
    let mut out = Vec::new();
    for kind in KINDS {
        let (source, adapter, adapter_version) = match installed_entry(kind) {
            Some((path, version)) => (Some("enowx"), Some(path), Some(version)),
            None => match which(kind.bin) {
                Some(path) => (Some("path"), Some(path), None),
                None => (None, None, None),
            },
        };
        let cli = which(kind.cli);
        let login = match &cli {
            Some(_) => login_status(kind).await.ok(),
            None => None,
        };
        out.push(Detection {
            id: kind.id.into(),
            title: kind.title.into(),
            node: node.clone(),
            source,
            adapter: adapter.map(|p| p.display().to_string()),
            adapter_version,
            wanted_version: Some(kind.version),
            cli_version: cli.as_deref().and_then(version_of),
            cli: cli.map(|p| p.display().to_string()),
            login,
        });
    }
    for (name, custom) in &config.custom {
        out.push(Detection {
            id: name.clone(),
            title: name.clone(),
            node: node.clone(),
            source: Some("custom"),
            adapter: which(&custom.command).map(|p| p.display().to_string()),
            ..Detection::default()
        });
    }
    out
}

/// Install `kind`'s adapter into `~/.enx/acp` with npm, reporting each line
/// of npm's output. Never global.
pub fn install(kind: &Kind, mut progress: impl FnMut(&str)) -> Result<(), String> {
    use std::io::BufRead;
    let npm = which("npm").ok_or(
        "Node.js with npm is needed to run this agent. Install Node.js 20 or newer from \
         nodejs.org, then try again.",
    )?;
    let dir = install_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if !dir.join("package.json").exists() {
        std::fs::write(dir.join("package.json"), "{\"private\":true}\n")
            .map_err(|e| e.to_string())?;
    }
    progress(&format!("installing {}@{}…", kind.package, kind.version));
    let mut child = std::process::Command::new(npm)
        .args([
            "install",
            "--no-audit",
            "--no-fund",
            "--save-exact",
            "--loglevel",
            "http",
        ])
        .arg(format!("{}@{}", kind.package, kind.version))
        .current_dir(&dir)
        .env("PATH", login_path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            for line in std::io::BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
            {
                let _ = tx.send(line);
            }
        }));
    }
    drop(tx);
    let mut tail: Vec<String> = Vec::new();
    for line in rx {
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        progress(&line);
        tail.push(line);
        if tail.len() > 8 {
            tail.remove(0);
        }
    }
    for reader in readers {
        let _ = reader.join();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "installing {} failed: {}",
            kind.title,
            tail.last().cloned().unwrap_or_default()
        ))
    }
}

/// How to start an engine.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// How to start `id`, or why it cannot be.
pub fn launch(config: &AcpConfig, id: &str) -> Result<Launch, String> {
    let mut env = vec![("PATH".to_owned(), login_path().to_owned())];
    if let Some(custom) = config.custom.get(id) {
        let program = which(&custom.command)
            .ok_or_else(|| format!("`{}` was not found for {id}", custom.command))?;
        env.extend(custom.env.iter().map(|(k, v)| (k.clone(), v.clone())));
        return Ok(Launch {
            program,
            args: custom.args.clone(),
            env,
        });
    }
    let kind = kind(id).ok_or_else(|| format!("unknown agent engine `{id}`"))?;
    let args: Vec<String> = kind.args.iter().map(|a| (*a).to_owned()).collect();
    if let Some((entry, _)) = installed_entry(kind) {
        let node = which("node").ok_or("Node.js is needed to run this agent")?;
        let mut all = vec![entry.display().to_string()];
        all.extend(args);
        return Ok(Launch {
            program: node,
            args: all,
            env,
        });
    }
    if let Some(bin) = which(kind.bin) {
        return Ok(Launch {
            program: bin,
            args,
            env,
        });
    }
    Err(format!(
        "{} is not set up: install it in Settings > ACP agents",
        kind.title
    ))
}

/// Whether the user is signed in to an engine's CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginStatus {
    pub signed_in: bool,
    /// What they are signed in with ("Claude Max", "ChatGPT"); never an
    /// email address.
    pub label: Option<String>,
}

/// Ask the CLI whether the user is signed in. Local: it reads the CLI's own
/// stored credentials and starts no model turn.
pub async fn login_status(kind: &Kind) -> Result<LoginStatus, String> {
    let args: &[&str] = match kind.id {
        "claude" => &["auth", "status"],
        "codex" => &["login", "status"],
        _ => return Err("this CLI has no sign-in check".into()),
    };
    let cli = which(kind.cli).ok_or("not installed")?;
    let run = tokio::process::Command::new(cli)
        .args(args)
        .env("PATH", login_path())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), run)
        .await
        .map_err(|_| "no answer".to_owned())?
        .map_err(|e| e.to_string())?;
    Ok(parse_login(
        kind.id,
        out.status.success(),
        &String::from_utf8_lossy(&out.stdout),
        &String::from_utf8_lossy(&out.stderr),
    ))
}

pub fn parse_login(id: &str, ok: bool, stdout: &str, stderr: &str) -> LoginStatus {
    if id == "claude" {
        if let Ok(v) = serde_json::from_str::<Value>(stdout) {
            let signed_in = v.get("loggedIn").and_then(Value::as_bool).unwrap_or(false);
            let label = v.get("subscriptionType").and_then(Value::as_str).map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    Some(first) => format!("Claude {}{}", first.to_uppercase(), chars.as_str()),
                    None => "Claude".into(),
                }
            });
            return LoginStatus {
                signed_in,
                label: label.filter(|_| signed_in),
            };
        }
    }
    let text = format!("{stdout}\n{stderr}");
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    let signed_in = ok && !line.to_lowercase().contains("not logged in");
    LoginStatus {
        signed_in,
        label: signed_in
            .then(|| {
                line.strip_prefix("Logged in using ")
                    .unwrap_or(line)
                    .to_owned()
            })
            .filter(|l| !l.contains('@') && !l.is_empty()),
    }
}
