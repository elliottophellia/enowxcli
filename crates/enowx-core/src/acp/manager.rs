//! The running engines, kept warm between turns, and which agent session
//! each enowx session is talking to.
//!
//! One process per engine serves every enowx session on it; each enowx
//! session (and agent) gets its own agent session, created on its first turn
//! and kept for the next, so the agent remembers the conversation and the
//! second message does not pay for a cold start.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use super::client::{self, Process, Sink};
use super::mcp_host::Route;
use super::AcpConfig;

/// What an engine sends a turn.
pub enum Incoming {
    Update(Value),
    Permission(Value, oneshot::Sender<Option<String>>),
    Exited(String),
}

/// Routes one engine's messages to the turn listening for their session.
#[derive(Default)]
struct Router {
    listeners: Mutex<HashMap<String, mpsc::UnboundedSender<Incoming>>>,
}

fn session_of(params: &Value) -> String {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

impl Sink for Router {
    fn update(&self, params: Value) {
        let id = session_of(&params);
        if let Some(tx) = self.listeners.lock().ok().and_then(|l| l.get(&id).cloned()) {
            let _ = tx.send(Incoming::Update(params));
        }
    }

    fn permission(&self, params: Value, reply: oneshot::Sender<Option<String>>) {
        let id = session_of(&params);
        match self.listeners.lock().ok().and_then(|l| l.get(&id).cloned()) {
            Some(tx) => {
                if let Err(mpsc::error::SendError(Incoming::Permission(_, reply))) =
                    tx.send(Incoming::Permission(params, reply))
                {
                    let _ = reply.send(None);
                }
            }
            // No turn is running for it: nobody to ask.
            None => {
                let _ = reply.send(None);
            }
        }
    }

    fn exited(&self, detail: String) {
        let listeners: Vec<_> = self
            .listeners
            .lock()
            .map(|l| l.values().cloned().collect())
            .unwrap_or_default();
        for tx in listeners {
            let _ = tx.send(Incoming::Exited(detail.clone()));
        }
    }
}

/// A running engine.
pub struct Engine {
    pub id: String,
    pub process: Arc<Process>,
    /// Its `initialize` answer: capabilities, auth methods.
    pub init: Value,
    pub generation: u64,
    router: Arc<Router>,
}

impl Engine {
    /// Messages for `session` until [`Engine::stop_listening`].
    pub fn listen(&self, session: &str) -> mpsc::UnboundedReceiver<Incoming> {
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut l) = self.router.listeners.lock() {
            l.insert(session.to_owned(), tx);
        }
        rx
    }

    pub fn stop_listening(&self, session: &str) {
        if let Ok(mut l) = self.router.listeners.lock() {
            l.remove(session);
        }
    }

    /// Whether it can load a saved session.
    pub fn can_load(&self) -> bool {
        self.init
            .pointer("/agentCapabilities/loadSession")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Whether prompts may carry images.
    pub fn takes_images(&self) -> bool {
        self.init
            .pointer("/agentCapabilities/promptCapabilities/image")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}

/// An enowx session's agent session on an engine.
#[derive(Clone)]
pub struct Bound {
    pub acp_session: String,
    pub route: Arc<Route>,
    pub generation: u64,
}

#[derive(Default)]
pub struct Manager {
    engines: Mutex<HashMap<String, Arc<Engine>>>,
    sessions: Mutex<HashMap<String, Bound>>,
}

/// The one manager for this process: engines stay warm across turns and
/// across the agent being rebuilt after a settings change.
pub fn manager() -> &'static Manager {
    static MANAGER: OnceLock<Manager> = OnceLock::new();
    MANAGER.get_or_init(Manager::default)
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

impl Manager {
    /// The engine `id`, started and initialised if it is not running.
    pub async fn engine(
        &self,
        config: &AcpConfig,
        id: &str,
        cwd: &Path,
    ) -> Result<Arc<Engine>, String> {
        if let Some(engine) = self
            .engines
            .lock()
            .ok()
            .and_then(|e| e.get(id).cloned())
            .filter(|e| e.process.is_alive())
        {
            return Ok(engine);
        }
        let launch = super::launch(config, id)?;
        let router = Arc::new(Router::default());
        let process = Process::spawn(&launch, cwd, router.clone()).map_err(|e| e.to_string())?;
        let init = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            process.request("initialize", client::initialize_params()),
        )
        .await
        {
            Ok(Ok(init)) => init,
            Ok(Err(error)) => {
                process.kill();
                return Err(format!("{} did not start: {error}", super::title(id)));
            }
            Err(_) => {
                process.kill();
                return Err(format!("{} did not answer within 60 s", super::title(id)));
            }
        };
        let engine = Arc::new(Engine {
            id: id.to_owned(),
            process,
            init,
            generation: GENERATION.fetch_add(1, Ordering::Relaxed) + 1,
            router,
        });
        if let Ok(mut engines) = self.engines.lock() {
            engines.insert(id.to_owned(), engine.clone());
        }
        Ok(engine)
    }

    /// The agent session bound to `key`, while its engine still runs.
    pub fn bound(&self, key: &str, engine: &Engine) -> Option<Bound> {
        self.sessions
            .lock()
            .ok()?
            .get(key)
            .filter(|b| b.generation == engine.generation)
            .cloned()
    }

    pub fn bind(&self, key: &str, bound: Bound) {
        if let Ok(mut s) = self.sessions.lock() {
            s.insert(key.to_owned(), bound);
        }
    }

    /// Stop one engine, as after its settings change.
    pub fn stop(&self, id: &str) {
        if let Some(engine) = self.engines.lock().ok().and_then(|mut e| e.remove(id)) {
            engine.process.kill();
        }
    }

    /// Stop every engine: enowx is exiting.
    pub fn stop_all(&self) {
        let engines: Vec<_> = self
            .engines
            .lock()
            .map(|mut e| e.drain().map(|(_, v)| v).collect())
            .unwrap_or_default();
        for engine in engines {
            engine.process.kill();
        }
    }
}
