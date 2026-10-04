//! One ACP agent process and its JSON-RPC connection: newline-delimited
//! JSON-RPC 2.0 over stdio, protocol version 1.
//!
//! Written against plain JSON values rather than a typed SDK: enowx needs a
//! handful of methods, adapters add fields under `_meta` freely, and a
//! tolerant reader keeps working when they do.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use super::Launch;

pub const PROTOCOL_VERSION: u64 = 1;

/// What the process reports to whoever runs it.
pub trait Sink: Send + Sync + 'static {
    /// A `session/update` notification, as sent.
    fn update(&self, params: Value);
    /// The agent asks to be allowed something. Answer through `reply`:
    /// `Some(option_id)` picks an option, `None` cancels.
    fn permission(&self, params: Value, reply: oneshot::Sender<Option<String>>);
    /// The process ended.
    fn exited(&self, detail: String);
}

/// A failed request.
#[derive(Debug, Clone, thiserror::Error)]
pub enum AcpError {
    #[error("could not start the agent: {0}")]
    Spawn(String),
    #[error("the agent stopped{0}")]
    Exited(String),
    #[error("{message}")]
    Rpc { code: i64, message: String },
}

impl AcpError {
    /// ACP reserves -32000 for "authentication required".
    pub fn needs_login(&self) -> bool {
        match self {
            AcpError::Rpc { code, message } => {
                *code == -32000
                    || (message.to_lowercase().contains("auth")
                        && (message.contains("required") || message.contains("login")))
            }
            _ => false,
        }
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, AcpError>>>>>;

/// A running agent.
pub struct Process {
    outgoing: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl Process {
    pub fn spawn(
        launch: &Launch,
        cwd: &std::path::Path,
        sink: Arc<dyn Sink>,
    ) -> Result<Arc<Self>, AcpError> {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // enowx exiting must not leave an agent running on the user's
            // subscription with no one listening.
            .kill_on_drop(true);
        for (k, v) in &launch.env {
            command.env(k, v);
        }
        let mut child = command
            .spawn()
            .map_err(|e| AcpError::Spawn(format!("{}: {e}", launch.program.display())))?;
        let stdin = child
            .stdin
            .take()
            .ok_or(AcpError::Spawn("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or(AcpError::Spawn("no stdout".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or(AcpError::Spawn("no stderr".into()))?;

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let process = Arc::new(Process {
            outgoing: tx,
            pending: Arc::default(),
            next_id: AtomicU64::new(0),
            alive: Arc::new(AtomicBool::new(true)),
            child: Arc::new(Mutex::new(Some(child))),
        });

        // One task owns stdin, so two messages never interleave.
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    break;
                }
            }
        });

        // Stderr is kept briefly so a crash can say why, and never logged:
        // an adapter may print account details there.
        let tail: Arc<Mutex<VecDeque<String>>> = Arc::default();
        {
            let tail = tail.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if let Ok(mut t) = tail.lock() {
                        t.push_back(line);
                        while t.len() > 8 {
                            t.pop_front();
                        }
                    }
                }
            });
        }

        {
            let me = process.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() {
                        continue;
                    }
                    // Not JSON: an adapter printing to stdout by mistake.
                    if let Ok(message) = serde_json::from_str::<Value>(&line) {
                        me.dispatch(message, &sink);
                    }
                }
                me.on_exit(&sink, &tail).await;
            });
        }
        Ok(process)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Send a request and wait for its answer.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, AcpError> {
        if !self.is_alive() {
            return Err(AcpError::Exited(String::new()));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        if let Ok(mut pending) = self.pending.lock() {
            pending.insert(id, tx);
        }
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        rx.await
            .unwrap_or_else(|_| Err(AcpError::Exited(String::new())))
    }

    pub fn notify(&self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn send(&self, message: Value) {
        let _ = self.outgoing.send(message.to_string());
    }

    fn respond(&self, id: Value, result: Result<Value, (i64, String)>) {
        self.send(match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": code, "message": message }
            }),
        });
    }

    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.start_kill();
            }
        }
    }

    fn dispatch(self: &Arc<Self>, message: Value, sink: &Arc<dyn Sink>) {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let id = message.get("id").cloned().filter(|v| !v.is_null());
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match (method, id) {
            (None, Some(id)) => {
                let Some(id) = id.as_u64() else { return };
                let Some(tx) = self.pending.lock().ok().and_then(|mut p| p.remove(&id)) else {
                    return;
                };
                let result = match message.get("error") {
                    Some(error) => {
                        let mut message = error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("the agent reported an error")
                            .to_owned();
                        // The reason is often only in `data` ("Internal
                        // error" alone says nothing).
                        if let Some(data) = error.get("data").and_then(Value::as_str) {
                            message = format!("{message}: {data}");
                        }
                        Err(AcpError::Rpc {
                            code: error.get("code").and_then(Value::as_i64).unwrap_or(-32603),
                            message,
                        })
                    }
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = tx.send(result);
            }
            (Some(method), Some(id)) if method == "session/request_permission" => {
                let (tx, rx) = oneshot::channel();
                sink.permission(params, tx);
                let me = self.clone();
                tokio::spawn(async move {
                    let outcome = match rx.await.ok().flatten() {
                        Some(option) => json!({ "outcome": "selected", "optionId": option }),
                        None => json!({ "outcome": "cancelled" }),
                    };
                    me.respond(id, Ok(json!({ "outcome": outcome })));
                });
            }
            // File system and terminal are not offered: the agent uses its own.
            (Some(method), Some(id)) => {
                self.respond(id, Err((-32601, format!("method not found: {method}"))));
            }
            (Some(method), None) if method == "session/update" => sink.update(params),
            _ => {}
        }
    }

    async fn on_exit(&self, sink: &Arc<dyn Sink>, tail: &Arc<Mutex<VecDeque<String>>>) {
        self.alive.store(false, Ordering::SeqCst);
        let child = self.child.lock().ok().and_then(|mut c| c.take());
        let status = match child {
            Some(mut c) => c.wait().await.ok(),
            None => None,
        };
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let mut detail = status
            .and_then(|s| s.code())
            .map(|code| format!(" (exit code {code})"))
            .unwrap_or_default();
        let last = tail
            .lock()
            .ok()
            .and_then(|t| t.iter().rev().find(|l| !l.trim().is_empty()).cloned());
        if let Some(line) = last {
            detail.push_str(&format!(": {}", line.chars().take(300).collect::<String>()));
        }
        let waiting: Vec<_> = self
            .pending
            .lock()
            .map(|mut p| p.drain().collect())
            .unwrap_or_default();
        for (_, tx) in waiting {
            let _ = tx.send(Err(AcpError::Exited(detail.clone())));
        }
        sink.exited(detail);
    }
}

/// What enowx tells an agent about itself. No file system or terminal: the
/// agent runs its own tools, behind its own permission prompts, which come to
/// enowx.
pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false
        },
        "clientInfo": {
            "name": "enowx",
            "title": "enowx",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}
