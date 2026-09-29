//! Language servers, for type errors and lint warnings while the agent writes
//! code rather than when a build or test runs later.
//!
//! After `write`, `edit` or `multi_edit`, the file goes to the language server
//! for its language (rust-analyzer, typescript-language-server, pyright, ruff,
//! gopls) and what it reports comes back with the tool result. Servers start
//! on first use, one per language and project root, and live as long as the
//! agent. A server that is not installed is named once, with its install
//! command, and the edit goes on without it.
//!
//! An edit waits only for the server's first answer: rust-analyzer's
//! `cargo clippy` pass can take far longer than an edit should. The
//! `diagnostics` tool waits until the server has finished checking.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Notify};

/// A language server enx knows how to start.
pub struct Server {
    pub name: &'static str,
    pub bin: &'static str,
    pub args: &'static [&'static str],
    /// Arguments that print a version and exit cleanly when the server really
    /// runs. rustup installs a `rust-analyzer` that fails until the component
    /// is added, so being on PATH is not enough. Empty: PATH is enough.
    pub probe: &'static [&'static str],
    pub extensions: &'static [&'static str],
    /// Files that mark the root this server should be started in.
    pub root_markers: &'static [&'static str],
    pub install: &'static str,
}

pub const SERVERS: &[Server] = &[
    Server {
        name: "rust-analyzer",
        bin: "rust-analyzer",
        args: &[],
        probe: &["--version"],
        extensions: &["rs"],
        root_markers: &["Cargo.toml"],
        install: "rustup component add rust-analyzer",
    },
    Server {
        name: "typescript-language-server",
        bin: "typescript-language-server",
        args: &["--stdio"],
        probe: &["--version"],
        extensions: &["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"],
        root_markers: &["tsconfig.json", "jsconfig.json", "package.json"],
        install: "npm install -g typescript typescript-language-server",
    },
    Server {
        name: "pyright",
        bin: "pyright-langserver",
        args: &["--stdio"],
        probe: &[],
        extensions: &["py", "pyi"],
        root_markers: &["pyproject.toml", "pyrightconfig.json", "setup.py"],
        install: "npm install -g pyright",
    },
    Server {
        name: "ruff",
        bin: "ruff",
        args: &["server"],
        probe: &["--version"],
        extensions: &["py", "pyi"],
        root_markers: &["pyproject.toml", "ruff.toml", ".ruff.toml"],
        install: "pip install ruff",
    },
    Server {
        name: "gopls",
        bin: "gopls",
        args: &[],
        probe: &["version"],
        extensions: &["go"],
        root_markers: &["go.work", "go.mod"],
        install: "go install golang.org/x/tools/gopls@latest",
    },
];

/// The servers for a file, by its extension.
pub fn servers_for(path: &Path) -> Vec<&'static Server> {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Vec::new();
    };
    SERVERS
        .iter()
        .filter(|s| s.extensions.contains(&ext))
        .collect()
}

fn language_id(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "jsx" => "javascriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        _ => "plaintext",
    }
}

/// What rust-analyzer is told, at start and whenever it asks: check on save
/// with clippy, so lint warnings come with the type errors.
fn settings(server: &Server) -> Value {
    match server.name {
        "rust-analyzer" => json!({ "checkOnSave": true, "check": { "command": "clippy" } }),
        _ => Value::Null,
    }
}

pub fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn uri_path(uri: &str) -> PathBuf {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

/// The nearest directory from the file up to the workspace holding one of the
/// server's markers, or the workspace itself.
fn root_for(server: &Server, workspace: &Path, file: &Path) -> PathBuf {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if !d.starts_with(workspace) {
            break;
        }
        if server.root_markers.iter().any(|m| d.join(m).exists()) {
            return d.to_path_buf();
        }
        if d == workspace {
            break;
        }
        dir = d.parent();
    }
    workspace.to_path_buf()
}

// ---------------------------------------------------------------------------
// One running server

#[derive(Default)]
struct State {
    pending: HashMap<i64, oneshot::Sender<std::result::Result<Value, String>>>,
    /// Latest diagnostics per document, with the generation they arrived in.
    published: HashMap<String, (u64, Vec<Value>)>,
    /// Work the server has said it is doing (`$/progress` begin without end).
    busy: HashSet<String>,
    last_event: Option<Instant>,
}

struct Shared {
    state: Mutex<State>,
    generation: AtomicU64,
    alive: AtomicBool,
    changed: Notify,
}

impl Shared {
    fn touch(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.last_event = Some(Instant::now());
        }
        self.changed.notify_waiters();
    }
}

struct Client {
    server: &'static Server,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    shared: Arc<Shared>,
    next_id: AtomicI64,
    /// Open documents and their version.
    open: tokio::sync::Mutex<HashMap<String, i32>>,
    _child: Child,
}

async fn send(stdin: &tokio::sync::Mutex<ChildStdin>, message: &Value) -> Result<()> {
    let body = serde_json::to_vec(message)?;
    let mut stdin = stdin.lock().await;
    stdin
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    stdin.write_all(&body).await?;
    stdin.flush().await?;
    Ok(())
}

impl Client {
    async fn start(server: &'static Server, root: &Path) -> Result<Self> {
        let mut child = Command::new(server.bin)
            .args(server.args)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("start {}", server.bin))?;
        let stdin = Arc::new(tokio::sync::Mutex::new(
            child.stdin.take().ok_or_else(|| anyhow!("no stdin"))?,
        ));
        let stdout = child.stdout.take().ok_or_else(|| anyhow!("no stdout"))?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            generation: AtomicU64::new(0),
            alive: AtomicBool::new(true),
            changed: Notify::new(),
        });
        tokio::spawn(read_loop(
            server,
            BufReader::new(stdout),
            stdin.clone(),
            shared.clone(),
            root.to_path_buf(),
        ));
        let client = Self {
            server,
            stdin,
            shared,
            next_id: AtomicI64::new(1),
            open: tokio::sync::Mutex::new(HashMap::new()),
            _child: child,
        };
        let root_uri = file_uri(root);
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        client
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "rootPath": root,
                    "workspaceFolders": [{ "uri": root_uri, "name": name }],
                    "initializationOptions": settings(server),
                    "capabilities": {
                        "textDocument": {
                            "synchronization": { "didSave": true, "dynamicRegistration": false },
                            "publishDiagnostics": { "relatedInformation": false, "versionSupport": true },
                            "definition": { "linkSupport": false },
                            "references": {},
                            "hover": { "contentFormat": ["markdown", "plaintext"] },
                            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
                            "rename": { "prepareSupport": false }
                        },
                        "workspace": { "configuration": true, "workspaceFolders": true },
                        "window": { "workDoneProgress": true }
                    }
                }),
                Duration::from_secs(60),
            )
            .await
            .with_context(|| format!("{} did not start", server.name))?;
        client.notify("initialized", json!({})).await?;
        Ok(client)
    }

    fn alive(&self) -> bool {
        self.shared.alive.load(Ordering::SeqCst)
    }

    async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        if let Ok(mut state) = self.shared.state.lock() {
            state.pending.insert(id, tx);
        }
        send(
            &self.stdin,
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
        )
        .await?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(error))) => Err(anyhow!("{method}: {error}")),
            Ok(Err(_)) => Err(anyhow!("{} stopped", self.server.name)),
            Err(_) => Err(anyhow!("{method}: no answer in {}s", timeout.as_secs())),
        }
    }

    async fn notify(&self, method: &str, params: Value) -> Result<()> {
        send(
            &self.stdin,
            &json!({ "jsonrpc": "2.0", "method": method, "params": params }),
        )
        .await
    }

    /// Make the server's copy of a document match `text`.
    async fn sync(&self, path: &Path, text: &str) -> Result<()> {
        let uri = file_uri(path);
        let mut open = self.open.lock().await;
        match open.get_mut(&uri) {
            Some(version) => {
                *version += 1;
                self.notify(
                    "textDocument/didChange",
                    json!({
                        "textDocument": { "uri": uri, "version": *version },
                        "contentChanges": [{ "text": text }]
                    }),
                )
                .await
            }
            None => {
                open.insert(uri.clone(), 1);
                self.notify(
                    "textDocument/didOpen",
                    json!({ "textDocument": {
                        "uri": uri, "languageId": language_id(path), "version": 1, "text": text
                    }}),
                )
                .await
            }
        }
    }

    /// Send the file's current text and wait for what the server says about
    /// it: until it has answered and gone quiet, or until `deadline`.
    async fn check(&self, path: &Path, text: &str, deadline: Duration) -> Result<Check> {
        let uri = file_uri(path);
        let before = self.shared.generation.load(Ordering::SeqCst);
        {
            let mut open = self.open.lock().await;
            match open.get_mut(&uri) {
                Some(version) => {
                    *version += 1;
                    self.notify(
                        "textDocument/didChange",
                        json!({
                            "textDocument": { "uri": uri, "version": *version },
                            "contentChanges": [{ "text": text }]
                        }),
                    )
                    .await?;
                }
                None => {
                    open.insert(uri.clone(), 1);
                    self.notify(
                        "textDocument/didOpen",
                        json!({ "textDocument": {
                            "uri": uri, "languageId": language_id(path), "version": 1, "text": text
                        }}),
                    )
                    .await?;
                }
            }
        }
        self.notify(
            "textDocument/didSave",
            json!({ "textDocument": { "uri": uri }, "text": text }),
        )
        .await?;

        let started = Instant::now();
        let quiet = Duration::from_millis(400);
        loop {
            let notified = self.shared.changed.notified();
            let (answered, busy, last) = {
                let state = self
                    .shared
                    .state
                    .lock()
                    .map_err(|_| anyhow!("state poisoned"))?;
                (
                    state
                        .published
                        .get(&uri)
                        .is_some_and(|(generation, _)| *generation > before),
                    !state.busy.is_empty(),
                    state.last_event,
                )
            };
            let settled = last.is_none_or(|at| at.elapsed() >= quiet);
            if (answered && !busy && settled) || !self.alive() {
                break;
            }
            let left = deadline.saturating_sub(started.elapsed());
            if left.is_zero() {
                break;
            }
            let _ = tokio::time::timeout(left.min(Duration::from_millis(100)), notified).await;
        }

        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| anyhow!("state poisoned"))?;
        let answered = state
            .published
            .get(&uri)
            .is_some_and(|(generation, _)| *generation > before);
        let file = state
            .published
            .get(&uri)
            .map(|(_, items)| items.clone())
            .unwrap_or_default();
        // Errors a change caused elsewhere: another file the server
        // re-published while this one was checked.
        let mut elsewhere: Vec<(PathBuf, Vec<Value>)> = state
            .published
            .iter()
            .filter(|(other, (generation, items))| {
                *other != &uri && *generation > before && items.iter().any(is_error)
            })
            .map(|(other, (_, items))| (uri_path(other), items.clone()))
            .collect();
        elsewhere.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Check {
            answered,
            busy: !state.busy.is_empty() && self.alive(),
            file,
            elsewhere,
        })
    }
}

async fn read_loop(
    server: &'static Server,
    mut out: BufReader<tokio::process::ChildStdout>,
    stdin: Arc<tokio::sync::Mutex<ChildStdin>>,
    shared: Arc<Shared>,
    root: PathBuf,
) {
    loop {
        let mut length = None;
        loop {
            let mut line = String::new();
            match out.read_line(&mut line).await {
                Ok(0) | Err(_) => return stopped(&shared),
                Ok(_) => {}
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = value.trim().parse::<usize>().ok();
            }
        }
        let Some(length) = length else { continue };
        let mut body = vec![0; length];
        if out.read_exact(&mut body).await.is_err() {
            return stopped(&shared);
        }
        let Ok(message) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        let method = message.get("method").and_then(Value::as_str);
        match (method, message.get("id")) {
            // A response to one of ours.
            (None, Some(id)) => {
                let Some(id) = id.as_i64() else { continue };
                let waiter = shared
                    .state
                    .lock()
                    .ok()
                    .and_then(|mut state| state.pending.remove(&id));
                if let Some(waiter) = waiter {
                    let result = match message.get("error") {
                        Some(error) => Err(error.to_string()),
                        None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = waiter.send(result);
                }
            }
            // A request from the server: answer it, or it may wait forever.
            (Some(method), Some(id)) => {
                let result = match method {
                    "workspace/configuration" => {
                        let items = message["params"]["items"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default();
                        Value::Array(
                            items
                                .iter()
                                .map(|item| match item["section"].as_str() {
                                    Some(section) if section == server.name => settings(server),
                                    _ => Value::Null,
                                })
                                .collect(),
                        )
                    }
                    "workspace/workspaceFolders" => json!([{
                        "uri": file_uri(&root),
                        "name": root.file_name().map(|n| n.to_string_lossy().into_owned()),
                    }]),
                    _ => Value::Null,
                };
                let _ = send(
                    &stdin,
                    &json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                )
                .await;
            }
            (Some("textDocument/publishDiagnostics"), None) => {
                let params = &message["params"];
                if let Some(uri) = params["uri"].as_str() {
                    let generation = shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
                    let items = params["diagnostics"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if let Ok(mut state) = shared.state.lock() {
                        state.published.insert(uri.to_owned(), (generation, items));
                    }
                    shared.touch();
                }
            }
            (Some("$/progress"), None) => {
                let params = &message["params"];
                let token = match &params["token"] {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if let Ok(mut state) = shared.state.lock() {
                    match params["value"]["kind"].as_str() {
                        Some("begin") => {
                            state.busy.insert(token);
                        }
                        Some("end") => {
                            state.busy.remove(&token);
                        }
                        _ => {}
                    }
                }
                shared.touch();
            }
            _ => {}
        }
    }
}

fn stopped(shared: &Shared) {
    shared.alive.store(false, Ordering::SeqCst);
    if let Ok(mut state) = shared.state.lock() {
        state.pending.clear();
        state.busy.clear();
    }
    shared.changed.notify_waiters();
}

struct Check {
    answered: bool,
    busy: bool,
    file: Vec<Value>,
    elsewhere: Vec<(PathBuf, Vec<Value>)>,
}

fn is_error(item: &Value) -> bool {
    item["severity"].as_u64() == Some(1)
}

// ---------------------------------------------------------------------------
// The servers an agent keeps

/// How long a check waits.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wait {
    /// After an edit: the server's first answer, a few seconds at most.
    Edit,
    /// The `diagnostics` tool: until the server has finished checking.
    Full,
}

impl Wait {
    fn deadline(self, first_start: bool) -> Duration {
        match (self, first_start) {
            (Wait::Edit, false) => Duration::from_secs(4),
            // A server that just started indexes the project first.
            (Wait::Edit, true) => Duration::from_secs(12),
            (Wait::Full, _) => Duration::from_secs(90),
        }
    }
}

/// The language servers one agent has started, by server and project root.
pub struct Lsp {
    workspace: PathBuf,
    clients: tokio::sync::Mutex<HashMap<(&'static str, PathBuf), Arc<Client>>>,
    usable: Mutex<HashMap<&'static str, bool>>,
    /// Missing servers already named, so each is named once.
    named: Mutex<HashSet<&'static str>>,
}

impl Lsp {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            clients: tokio::sync::Mutex::new(HashMap::new()),
            usable: Mutex::new(HashMap::new()),
            named: Mutex::new(HashSet::new()),
        }
    }

    async fn is_usable(&self, server: &'static Server) -> bool {
        if let Some(known) = self
            .usable
            .lock()
            .ok()
            .and_then(|u| u.get(server.name).copied())
        {
            return known;
        }
        let usable = which::which(server.bin).is_ok()
            && (server.probe.is_empty()
                || matches!(
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        Command::new(server.bin)
                            .args(server.probe)
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .kill_on_drop(true)
                            .status(),
                    )
                    .await,
                    Ok(Ok(status)) if status.success()
                ));
        if let Ok(mut known) = self.usable.lock() {
            known.insert(server.name, usable);
        }
        usable
    }

    async fn client(&self, server: &'static Server, root: PathBuf) -> Result<(Arc<Client>, bool)> {
        let mut clients = self.clients.lock().await;
        let key = (server.name, root.clone());
        if let Some(client) = clients.get(&key) {
            if client.alive() {
                return Ok((client.clone(), false));
            }
        }
        let client = Arc::new(Client::start(server, &root).await?);
        clients.insert(key, client.clone());
        Ok((client, true))
    }

    /// What the language servers for `path` say about `text`, formatted for
    /// the model, or None when no server covers the file (or one that is
    /// missing was already named).
    pub async fn check(&self, path: &Path, text: &str, wait: Wait) -> Option<String> {
        let mut sections = Vec::new();
        for server in servers_for(path) {
            if !self.is_usable(server).await {
                let first = self
                    .named
                    .lock()
                    .map(|mut named| named.insert(server.name))
                    .unwrap_or(false);
                if first || wait == Wait::Full {
                    sections.push(format!(
                        "{} is not installed, so this file was not checked as you wrote it \
                         (install: {}).",
                        server.name, server.install
                    ));
                }
                continue;
            }
            let root = root_for(server, &self.workspace, path);
            let result = match self.client(server, root).await {
                Ok((client, fresh)) => client.check(path, text, wait.deadline(fresh)).await,
                Err(error) => Err(error),
            };
            sections.push(match result {
                Ok(check) => self.describe(server, path, &check),
                Err(error) => format!("{}: {error:#}", server.name),
            });
        }
        (!sections.is_empty()).then(|| sections.join("\n"))
    }

    fn describe(&self, server: &Server, path: &Path, check: &Check) -> String {
        let shown = |p: &Path| {
            p.strip_prefix(&self.workspace)
                .unwrap_or(p)
                .display()
                .to_string()
        };
        if !check.answered {
            return format!(
                "{} has not reported on {} yet{}; call `diagnostics` on it for the result.",
                server.name,
                shown(path),
                if check.busy { " (still checking)" } else { "" }
            );
        }
        let errors = check.file.iter().filter(|d| is_error(d)).count();
        let warnings = check
            .file
            .iter()
            .filter(|d| d["severity"].as_u64() == Some(2))
            .count();
        let mut out = if errors + warnings == 0 {
            format!("{}: no errors or warnings in {}", server.name, shown(path))
        } else {
            format!(
                "{}: {} in {}",
                server.name,
                counts(errors, warnings),
                shown(path)
            )
        };
        const LIMIT: usize = 20;
        let mut rows = 0;
        let mut add = |out: &mut String, file: &Path, items: &[Value], only_errors: bool| {
            for item in items {
                let severity = item["severity"].as_u64().unwrap_or(1);
                if severity > 2 || (only_errors && severity != 1) {
                    continue;
                }
                if rows == LIMIT {
                    out.push_str("\n  …");
                    rows += 1;
                    return;
                }
                if rows > LIMIT {
                    return;
                }
                out.push_str(&format!("\n  {}", line(&shown(file), item)));
                rows += 1;
            }
        };
        add(&mut out, path, &check.file, false);
        for (file, items) in &check.elsewhere {
            add(&mut out, file, items, true);
        }
        if check.busy {
            out.push_str(&format!(
                "\n  ({} is still checking; call `diagnostics` on this file for the rest)",
                server.name
            ));
        }
        out
    }
}

fn counts(errors: usize, warnings: usize) -> String {
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (errors, warnings) {
        (0, w) => plural(w, "warning"),
        (e, 0) => plural(e, "error"),
        (e, w) => format!("{}, {}", plural(e, "error"), plural(w, "warning")),
    }
}

/// `src/main.rs:12:5 error: mismatched types [E0308]`, 1-based.
fn line(file: &str, item: &Value) -> String {
    let start = &item["range"]["start"];
    let row = start["line"].as_u64().unwrap_or(0) + 1;
    let column = start["character"].as_u64().unwrap_or(0) + 1;
    let severity = if is_error(item) { "error" } else { "warning" };
    let message: String = item["message"]
        .as_str()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(200)
        .collect();
    let code = match &item["code"] {
        Value::String(code) => format!(" [{code}]"),
        Value::Number(code) => format!(" [{code}]"),
        _ => String::new(),
    };
    format!("{file}:{row}:{column} {severity}: {message}{code}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip_paths_with_spaces() {
        let path = Path::new("/Volumes/SSD/my project/src/main.rs");
        let uri = file_uri(path);
        assert_eq!(uri, "file:///Volumes/SSD/my%20project/src/main.rs");
        assert_eq!(uri_path(&uri), path);
    }

    #[test]
    fn a_file_goes_to_the_servers_for_its_language() {
        let names =
            |p: &str| -> Vec<&str> { servers_for(Path::new(p)).iter().map(|s| s.name).collect() };
        assert_eq!(names("src/lib.rs"), ["rust-analyzer"]);
        assert_eq!(names("app/page.tsx"), ["typescript-language-server"]);
        assert_eq!(names("main.py"), ["pyright", "ruff"]);
        assert_eq!(names("main.go"), ["gopls"]);
        assert!(names("README.md").is_empty());
    }

    #[test]
    fn a_diagnostic_reads_as_one_line() {
        let item = json!({
            "range": { "start": { "line": 11, "character": 4 } },
            "severity": 1,
            "message": "mismatched types\nexpected `u32`",
            "code": "E0308"
        });
        assert_eq!(
            line("src/main.rs", &item),
            "src/main.rs:12:5 error: mismatched types [E0308]"
        );
        assert_eq!(counts(2, 1), "2 errors, 1 warning");
        assert_eq!(counts(0, 3), "3 warnings");
    }
}

// ---------------------------------------------------------------------------
// Asking the server about code: where a symbol is defined, where it is used,
// what it is, what a file declares, and renaming it everywhere.

/// What the `lsp` tool can ask.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ask {
    Definition,
    References,
    Hover,
    Symbols,
    Rename,
}

impl Ask {
    pub fn parse(op: &str) -> Option<Self> {
        Some(match op {
            "definition" => Ask::Definition,
            "references" => Ask::References,
            "hover" => Ask::Hover,
            "symbols" => Ask::Symbols,
            "rename" => Ask::Rename,
            _ => return None,
        })
    }
}

/// A 0-based position in a file, from a 1-based line and either a column or
/// the symbol's name on that line.
pub fn position(
    text: &str,
    line: usize,
    column: Option<usize>,
    symbol: Option<&str>,
) -> Option<(u32, u32)> {
    let row = line.checked_sub(1)?;
    let source = text.lines().nth(row)?;
    let byte = match (symbol, column) {
        (Some(name), _) if !name.is_empty() => source.find(name)?,
        (_, Some(column)) => source
            .char_indices()
            .nth(column.saturating_sub(1))
            .map_or(source.len(), |(i, _)| i),
        _ => source.len() - source.trim_start().len(),
    };
    // LSP counts UTF-16 units.
    let character = source[..byte].encode_utf16().count();
    Some((row as u32, character as u32))
}

impl Lsp {
    /// Ask the language server for `path` a question at a place in it.
    pub async fn ask(
        &self,
        path: &Path,
        ask: Ask,
        at: Option<(u32, u32)>,
        new_name: Option<&str>,
    ) -> Result<String> {
        let server = servers_for(path)
            .into_iter()
            .find(|s| s.name != "ruff")
            .ok_or_else(|| anyhow!("no language server covers {}", path.display()))?;
        if !self.is_usable(server).await {
            anyhow::bail!(
                "{} is not installed (install: {})",
                server.name,
                server.install
            );
        }
        let root = root_for(server, &self.workspace, path);
        let (client, fresh) = self.client(server, root).await?;
        let text = std::fs::read_to_string(path)?;
        client.sync(path, &text).await?;
        if fresh {
            // A server that just started indexes before it can answer about
            // other files; give it its first pass.
            let _ = client.check(path, &text, Duration::from_secs(20)).await;
        }
        let uri = file_uri(path);
        let doc = json!({ "uri": uri });
        let pos = |at: Option<(u32, u32)>| -> Result<Value> {
            let (line, character) =
                at.ok_or_else(|| anyhow!("give `line` and `symbol` (or `column`)"))?;
            Ok(json!({ "line": line, "character": character }))
        };
        let wait = Duration::from_secs(30);
        let shown = |p: &Path| {
            p.strip_prefix(&self.workspace)
                .unwrap_or(p)
                .display()
                .to_string()
        };
        match ask {
            Ask::Definition | Ask::References => {
                let method = if ask == Ask::Definition {
                    "textDocument/definition"
                } else {
                    "textDocument/references"
                };
                let mut params = json!({ "textDocument": doc, "position": pos(at)? });
                if ask == Ask::References {
                    params["context"] = json!({ "includeDeclaration": true });
                }
                let found = client.request(method, params, wait).await?;
                let list: Vec<Value> = match found {
                    Value::Array(items) => items,
                    Value::Null => Vec::new(),
                    one => vec![one],
                };
                if list.is_empty() {
                    return Ok("Nothing found.".to_owned());
                }
                let mut out = Vec::new();
                for item in list.iter().take(60) {
                    let (target, range) = match (item.get("targetUri"), item.get("uri")) {
                        (Some(uri), _) => (uri, &item["targetSelectionRange"]),
                        (None, Some(uri)) => (uri, &item["range"]),
                        _ => continue,
                    };
                    let file = uri_path(target.as_str().unwrap_or(""));
                    let line = range["start"]["line"].as_u64().unwrap_or(0) as usize;
                    let source = std::fs::read_to_string(&file)
                        .ok()
                        .and_then(|t| t.lines().nth(line).map(|l| l.trim().to_owned()))
                        .unwrap_or_default();
                    out.push(format!(
                        "{}:{}:{}  {}",
                        shown(&file),
                        line + 1,
                        range["start"]["character"].as_u64().unwrap_or(0) + 1,
                        source.chars().take(160).collect::<String>()
                    ));
                }
                if list.len() > 60 {
                    out.push(format!("… and {} more", list.len() - 60));
                }
                Ok(out.join("\n"))
            }
            Ask::Hover => {
                let found = client
                    .request(
                        "textDocument/hover",
                        json!({ "textDocument": doc, "position": pos(at)? }),
                        wait,
                    )
                    .await?;
                let contents = &found["contents"];
                let text = match contents {
                    Value::String(s) => s.clone(),
                    Value::Array(parts) => parts
                        .iter()
                        .map(|p| p["value"].as_str().or(p.as_str()).unwrap_or("").to_owned())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    other => other["value"].as_str().unwrap_or("").to_owned(),
                };
                Ok(if text.trim().is_empty() {
                    "Nothing to say about that place.".to_owned()
                } else {
                    text.chars().take(4000).collect()
                })
            }
            Ask::Symbols => {
                let found = client
                    .request(
                        "textDocument/documentSymbol",
                        json!({ "textDocument": doc }),
                        wait,
                    )
                    .await?;
                let mut out = Vec::new();
                fn walk(items: &[Value], depth: usize, out: &mut Vec<String>) {
                    for item in items {
                        let range = if item.get("selectionRange").is_some() {
                            &item["selectionRange"]
                        } else {
                            &item["location"]["range"]
                        };
                        out.push(format!(
                            "{}{} {} (line {})",
                            "  ".repeat(depth),
                            symbol_kind(item["kind"].as_u64().unwrap_or(0)),
                            item["name"].as_str().unwrap_or("?"),
                            range["start"]["line"].as_u64().unwrap_or(0) + 1
                        ));
                        if let Some(children) = item["children"].as_array() {
                            walk(children, depth + 1, out);
                        }
                    }
                }
                walk(
                    found.as_array().map(Vec::as_slice).unwrap_or(&[]),
                    0,
                    &mut out,
                );
                Ok(if out.is_empty() {
                    "No symbols.".to_owned()
                } else {
                    out.join("\n")
                })
            }
            Ask::Rename => {
                let new_name = new_name
                    .filter(|n| !n.trim().is_empty())
                    .ok_or_else(|| anyhow!("rename needs `new_name`"))?;
                let edit = client
                    .request(
                        "textDocument/rename",
                        json!({ "textDocument": doc, "position": pos(at)?, "newName": new_name }),
                        Duration::from_secs(60),
                    )
                    .await?;
                let changed = self.apply(&client, &edit)?;
                for (file, text) in &changed {
                    let _ = client.sync(file, text).await;
                }
                if changed.is_empty() {
                    return Ok(
                        "The server made no changes: is the position on a symbol?".to_owned()
                    );
                }
                let files: Vec<String> = changed.iter().map(|(f, _)| shown(f)).collect();
                Ok(format!(
                    "Renamed to `{new_name}` in {} files:\n{}",
                    files.len(),
                    files.join("\n")
                ))
            }
        }
    }

    /// Apply a workspace edit to files inside the workspace. Returns each
    /// changed file with its new text.
    fn apply(&self, _client: &Client, edit: &Value) -> Result<Vec<(PathBuf, String)>> {
        let mut by_file: Vec<(String, Vec<Value>)> = Vec::new();
        if let Some(changes) = edit["changes"].as_object() {
            for (uri, edits) in changes {
                by_file.push((uri.clone(), edits.as_array().cloned().unwrap_or_default()));
            }
        }
        if let Some(docs) = edit["documentChanges"].as_array() {
            for doc in docs {
                if let (Some(uri), Some(edits)) =
                    (doc["textDocument"]["uri"].as_str(), doc["edits"].as_array())
                {
                    by_file.push((uri.to_owned(), edits.clone()));
                }
            }
        }
        let mut changed = Vec::new();
        for (uri, mut edits) in by_file {
            let file = uri_path(&uri);
            if !file.starts_with(&self.workspace) {
                anyhow::bail!(
                    "the rename reaches outside the workspace: {}",
                    file.display()
                );
            }
            let mut text = std::fs::read_to_string(&file)?;
            // Last first, so earlier offsets stay right.
            edits.sort_by_key(|e| {
                std::cmp::Reverse((
                    e["range"]["start"]["line"].as_u64().unwrap_or(0),
                    e["range"]["start"]["character"].as_u64().unwrap_or(0),
                ))
            });
            for e in &edits {
                let start = offset(&text, &e["range"]["start"]);
                let end = offset(&text, &e["range"]["end"]);
                if let (Some(start), Some(end)) = (start, end) {
                    text.replace_range(start..end, e["newText"].as_str().unwrap_or(""));
                }
            }
            crate::config::atomic_write(&file, text.as_bytes())?;
            changed.push((file, text));
        }
        Ok(changed)
    }
}

/// The byte offset of an LSP position (line, UTF-16 character).
fn offset(text: &str, position: &Value) -> Option<usize> {
    let line = position["line"].as_u64()? as usize;
    let character = position["character"].as_u64()? as usize;
    let mut start = 0;
    for _ in 0..line {
        start += text[start..].find('\n')? + 1;
    }
    let row = text[start..].split('\n').next().unwrap_or("");
    let mut units = 0;
    for (i, c) in row.char_indices() {
        if units >= character {
            return Some(start + i);
        }
        units += c.len_utf16();
    }
    Some(start + row.len())
}

fn symbol_kind(kind: u64) -> &'static str {
    match kind {
        2 => "module",
        5 => "class",
        6 => "method",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        22 => "enum member",
        23 => "struct",
        26 => "type parameter",
        _ => "symbol",
    }
}
