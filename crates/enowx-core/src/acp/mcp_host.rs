//! enowx's tools for an ACP agent, as an MCP server.
//!
//! The agent brings its own file and shell tools; what it gets from enowx is
//! what only enowx has: skills, delegating to enowx's specialists, asking the
//! user through enowx. Each agent session has its own route, with its own
//! bearer token and tool list, and a call is handed to the enowx turn running
//! that session, which answers it.
//!
//! Transport is MCP's Streamable HTTP in its plain request and response form:
//! one POST per JSON-RPC message, answered with JSON. It listens on 127.0.0.1
//! only, on a port the OS picks, and a request without a known token is
//! refused.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Protocol revisions answered in. A client asking for one of these gets it
/// back; anything else is offered the newest.
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// How long a call may take. A delegation runs a whole specialist turn.
const CALL_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// A tool call for the turn to run. Answer on `reply` with an MCP
/// `CallToolResult`.
pub struct Call {
    pub name: String,
    pub arguments: Value,
    pub reply: std::sync::mpsc::Sender<Value>,
}

/// One agent session's view of enowx.
#[derive(Default)]
pub struct Route {
    tools: Mutex<Vec<Value>>,
    instructions: Mutex<String>,
    /// The turn running the session now; `None` between turns.
    turn: Mutex<Option<mpsc::UnboundedSender<Call>>>,
}

impl Route {
    /// Replace what the session is offered.
    pub fn offer(&self, tools: Vec<Value>, instructions: String) {
        if let Ok(mut t) = self.tools.lock() {
            *t = tools;
        }
        if let Ok(mut i) = self.instructions.lock() {
            *i = instructions;
        }
    }

    /// Hand calls to this turn until it ends.
    pub fn attach(&self, turn: mpsc::UnboundedSender<Call>) {
        if let Ok(mut t) = self.turn.lock() {
            *t = Some(turn);
        }
    }

    pub fn detach(&self) {
        if let Ok(mut t) = self.turn.lock() {
            *t = None;
        }
    }
}

pub struct Host {
    port: u16,
    routes: Mutex<HashMap<String, Arc<Route>>>,
}

/// The one server for this process, started on first use.
pub fn host() -> Result<Arc<Host>, String> {
    static HOST: OnceLock<Result<Arc<Host>, String>> = OnceLock::new();
    HOST.get_or_init(Host::start).clone()
}

impl Host {
    fn start() -> Result<Arc<Self>, String> {
        let server = tiny_http::Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = server
            .server_addr()
            .to_ip()
            .map(|a| a.port())
            .ok_or("no port")?;
        let host = Arc::new(Host {
            port,
            routes: Mutex::default(),
        });
        let me = host.clone();
        std::thread::Builder::new()
            .name("enowx-acp-mcp".into())
            .spawn(move || {
                for request in server.incoming_requests() {
                    let me = me.clone();
                    // A delegation blocks its request for minutes; a
                    // tools/list must not wait behind it.
                    std::thread::spawn(move || me.serve(request));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(host)
    }

    /// A new route, and the entry an ACP `session/new` lists under
    /// `mcpServers` to reach it.
    pub fn route(&self) -> (Arc<Route>, Value) {
        let token = random_token();
        let route = Arc::new(Route::default());
        if let Ok(mut routes) = self.routes.lock() {
            routes.insert(token.clone(), route.clone());
        }
        let entry = json!({
            "type": "http",
            "name": "enowx",
            "url": format!("http://127.0.0.1:{}/mcp", self.port),
            "headers": [{ "name": "Authorization", "value": format!("Bearer {token}") }]
        });
        (route, entry)
    }

    fn serve(&self, mut request: tiny_http::Request) {
        let respond = |request: tiny_http::Request, status: u16, body: Option<Value>| {
            let response = match body {
                Some(b) => tiny_http::Response::from_string(b.to_string())
                    .with_status_code(status)
                    .with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .expect("a static header"),
                    ),
                None => tiny_http::Response::from_string(String::new()).with_status_code(status),
            };
            let _ = request.respond(response);
        };
        if request.url().split('?').next() != Some("/mcp") {
            return respond(request, 404, None);
        }
        let token = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .and_then(|h| h.value.as_str().strip_prefix("Bearer "))
            .map(str::to_owned);
        let route = token.and_then(|t| self.routes.lock().ok()?.get(&t).cloned());
        let Some(route) = route else {
            return respond(request, 401, Some(json!({ "error": "unauthorized" })));
        };
        // A page in a browser was never handed the token; refuse any
        // cross-origin request outright all the same.
        if let Some(origin) = request.headers().iter().find(|h| h.field.equiv("Origin")) {
            let o = origin.value.as_str();
            if !(o.starts_with("http://127.0.0.1")
                || o.starts_with("http://localhost")
                || o == "null")
            {
                return respond(request, 403, None);
            }
        }
        match request.method() {
            tiny_http::Method::Post => {}
            tiny_http::Method::Delete => return respond(request, 200, None),
            _ => return respond(request, 405, None),
        }
        let mut body = String::new();
        if request.as_reader().read_to_string(&mut body).is_err() {
            return respond(request, 400, None);
        }
        let Ok(message) = serde_json::from_str::<Value>(&body) else {
            return respond(
                request,
                400,
                Some(json!({ "jsonrpc": "2.0", "id": null,
                             "error": { "code": -32700, "message": "parse error" } })),
            );
        };
        let reply = match &message {
            Value::Array(batch) => {
                let replies: Vec<Value> = batch.iter().filter_map(|m| handle(&route, m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            one => handle(&route, one),
        };
        match reply {
            Some(r) => respond(request, 200, Some(r)),
            None => respond(request, 202, None),
        }
    }
}

/// Answer one JSON-RPC message, or `None` for a notification.
pub fn handle(route: &Route, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned().filter(|v| !v.is_null())?;
    let method = message.get("method").and_then(Value::as_str)?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let result: Result<Value, (i64, String)> = match method {
        "initialize" => {
            let asked = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = if VERSIONS.contains(&asked) {
                asked
            } else {
                VERSIONS[0]
            };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "enowx", "title": "enowx", "version": env!("CARGO_PKG_VERSION") },
                "instructions": route.instructions.lock().map(|i| i.clone()).unwrap_or_default()
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": route.tools.lock().map(|t| t.clone()).unwrap_or_default()
        })),
        "tools/call" => Ok(call(route, &params)),
        other => Err((-32601, format!("method not found: {other}"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message }
        }),
    })
}

fn call(route: &Route, params: &Value) -> Value {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let names: Vec<String> = route
        .tools
        .lock()
        .map(|t| {
            t.iter()
                .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if !names.contains(&name) {
        return tool_error(&format!(
            "No such tool: {name}. Available: {}",
            names.join(", ")
        ));
    }
    let Some(turn) = route.turn.lock().ok().and_then(|t| t.clone()) else {
        return tool_error("enowx is not running a turn for this session right now.");
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let sent = turn.send(Call {
        name: name.clone(),
        arguments: params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({})),
        reply: tx,
    });
    if sent.is_err() {
        return tool_error("enowx's turn for this session has ended.");
    }
    rx.recv_timeout(CALL_TIMEOUT)
        .unwrap_or_else(|_| tool_error(&format!("{name} did not finish.")))
}

/// A tool result.
pub fn tool_text(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

pub fn tool_error(text: &str) -> Value {
    tool_text(text, true)
}

/// An OpenAI-style function schema as an MCP tool.
pub fn mcp_tool(function: &Value) -> Option<Value> {
    let f = function.get("function").unwrap_or(function);
    Some(json!({
        "name": f.get("name")?.as_str()?,
        "description": f.get("description").and_then(Value::as_str).unwrap_or_default(),
        "inputSchema": f.get("parameters").cloned().unwrap_or_else(|| json!({"type": "object"})),
    }))
}

/// 256 random bits, hex.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    let rng = ring::rand::SystemRandom::new();
    if ring::rand::SecureRandom::fill(&rng, &mut bytes).is_err() {
        // ring cannot fail on supported platforms; a UUID is the fallback.
        return uuid::Uuid::new_v4().simple().to_string()
            + &uuid::Uuid::new_v4().simple().to_string();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
