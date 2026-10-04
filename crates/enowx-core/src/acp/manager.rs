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

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use super::client::{self, Process, Sink};
use super::mcp_host::Route;
use super::{AcpConfig, EngineConfig};

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
    pub engine: String,
    pub acp_session: String,
    pub route: Arc<Route>,
    pub generation: u64,
}

#[derive(Default)]
pub struct Manager {
    engines: Mutex<HashMap<String, Arc<Engine>>>,
    sessions: Mutex<HashMap<String, Bound>>,
    /// The last `session/new` answer from each engine, by generation: what
    /// it offers (models, efforts, modes).
    created: Mutex<HashMap<String, (u64, Value)>>,
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

    /// Keep what an engine's `session/new` offered.
    pub fn remember(&self, engine: &Engine, created: &Value) {
        if let Ok(mut c) = self.created.lock() {
            c.insert(engine.id.clone(), (engine.generation, created.clone()));
        }
    }

    fn created_for(&self, engine: &Engine) -> Option<Value> {
        self.created
            .lock()
            .ok()?
            .get(&engine.id)
            .filter(|(generation, _)| *generation == engine.generation)
            .map(|(_, v)| v.clone())
    }

    /// The models and efforts `id` offers, starting it and opening a session
    /// (which runs no model) to ask when nothing is known yet.
    pub async fn offer(&self, config: &AcpConfig, id: &str, cwd: &Path) -> Result<Offer, String> {
        let engine = self.engine(config, id, cwd).await?;
        if let Some(created) = self.created_for(&engine) {
            return Ok(Offer::read(&created));
        }
        let mut params = json!({ "cwd": cwd.display().to_string(), "mcpServers": [] });
        if id == "claude" {
            params["_meta"] = json!({ "claudeCode": { "options": { "strictMcpConfig": true } } });
        }
        let created = engine
            .process
            .request("session/new", params)
            .await
            .map_err(|e| {
                if e.needs_login() {
                    format!(
                        "{} is not signed in. {}",
                        super::title(id),
                        super::kind(id).map_or("", |k| k.login_hint)
                    )
                } else {
                    format!("{}: {e}", super::title(id))
                }
            })?;
        self.remember(&engine, &created);
        Ok(Offer::read(&created))
    }

    /// Apply new settings to every live session on `id`, so a model picked
    /// mid-conversation takes effect from the next message.
    pub async fn reconfigure(&self, id: &str, settings: &EngineConfig) -> Vec<String> {
        let Some(engine) = self
            .engines
            .lock()
            .ok()
            .and_then(|e| e.get(id).cloned())
            .filter(|e| e.process.is_alive())
        else {
            return Vec::new();
        };
        let Some(created) = self.created_for(&engine) else {
            return Vec::new();
        };
        let sessions: Vec<String> = self
            .sessions
            .lock()
            .map(|s| {
                s.values()
                    .filter(|b| b.engine == id && b.generation == engine.generation)
                    .map(|b| b.acp_session.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut notes = Vec::new();
        for session in sessions {
            notes.extend(configure(&engine, &session, &created, settings).await);
        }
        notes.sort();
        notes.dedup();
        notes
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

/// Set a config option to the offered value that matches `wanted`.
pub async fn set_option(
    engine: &Engine,
    session: &str,
    options: &Value,
    ids: &[&str],
    wanted: &(dyn Fn(&str) -> bool + Sync),
) -> Option<String> {
    let option = options.as_array()?.iter().find(|o| {
        o.get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| ids.contains(&id))
    })?;
    let id = option.get("id")?.as_str()?;
    let value = choices(option)
        .into_iter()
        .find(|c| wanted(&c.value) || wanted(&c.name))?
        .value;
    let value = value.as_str();
    engine
        .process
        .request(
            "session/set_config_option",
            json!({ "sessionId": session, "configId": id, "value": value }),
        )
        .await
        .ok()?;
    Some(value.to_owned())
}

/// Apply the engine's settings to a new session. What could not be applied
/// is said, never silently dropped.
pub async fn configure(
    engine: &Engine,
    session: &str,
    created: &Value,
    settings: &EngineConfig,
) -> Vec<String> {
    let mut notes = Vec::new();
    let options = created.get("configOptions").cloned().unwrap_or(Value::Null);
    let model = settings.model.trim().to_lowercase();
    if !model.is_empty() {
        let exact = |v: &str| v.to_lowercase() == model;
        let matches = |v: &str| v.to_lowercase() == model || v.to_lowercase().contains(&model);
        let mut done = set_option(engine, session, &options, &["model"], &exact).await;
        if done.is_none() {
            done = set_option(engine, session, &options, &["model"], &matches).await;
        }
        if done.is_none() {
            // Older adapters list models apart from config options.
            let chosen = created
                .pointer("/models/availableModels")
                .and_then(Value::as_array)
                .and_then(|models| {
                    models.iter().find_map(|m| {
                        let id = m.get("modelId").and_then(Value::as_str)?;
                        let name = m.get("name").and_then(Value::as_str).unwrap_or(id);
                        (matches(id) || matches(name)).then(|| id.to_owned())
                    })
                });
            if let Some(id) = chosen {
                if engine
                    .process
                    .request(
                        "session/set_model",
                        json!({ "sessionId": session, "modelId": id }),
                    )
                    .await
                    .is_ok()
                {
                    done = Some(id);
                }
            }
        }
        if done.is_none() {
            notes.push(format!(
                "model `{}` is not one this agent offers; it uses its default",
                settings.model
            ));
        }
    }
    let effort = settings.effort.trim().to_lowercase();
    if !effort.is_empty()
        && set_option(
            engine,
            session,
            &options,
            &["effort", "reasoning_effort", "thought_level", "thinking"],
            &|v: &str| v.to_lowercase() == effort,
        )
        .await
        .is_none()
    {
        notes.push(format!(
            "effort `{}` is not one this agent offers",
            settings.effort
        ));
    }
    if settings.permission == "bypass" {
        let bypass = |v: &str| {
            let v = v.to_lowercase();
            v.contains("bypass") || v.contains("full") || v.contains("yolo")
        };
        let mut done = set_option(engine, session, &options, &["mode"], &bypass)
            .await
            .is_some();
        if !done {
            let mode = created
                .pointer("/modes/availableModes")
                .and_then(Value::as_array)
                .and_then(|modes| {
                    modes.iter().find_map(|m| {
                        let id = m.get("id").and_then(Value::as_str)?;
                        bypass(id).then(|| id.to_owned())
                    })
                });
            if let Some(id) = mode {
                done = engine
                    .process
                    .request(
                        "session/set_mode",
                        json!({ "sessionId": session, "modeId": id }),
                    )
                    .await
                    .is_ok();
            }
        }
        if !done {
            notes.push("this agent has no bypass mode; its prompts are allowed instead".into());
        }
    }
    notes
}

/// One value a config option offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub name: String,
}

/// Every value of a select option, groups flattened.
fn choices(option: &Value) -> Vec<Choice> {
    fn walk(list: &Value, out: &mut Vec<Choice>) {
        for item in list.as_array().into_iter().flatten() {
            if let Some(inner) = item.get("options") {
                walk(inner, out);
            } else if let Some(value) = item.get("value").and_then(Value::as_str) {
                out.push(Choice {
                    value: value.to_owned(),
                    name: item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or(value)
                        .to_owned(),
                });
            }
        }
    }
    let mut out = Vec::new();
    walk(option.get("options").unwrap_or(&Value::Null), &mut out);
    out
}

/// Config option ids an effort goes by.
pub const EFFORT_IDS: [&str; 4] = ["effort", "reasoning_effort", "thought_level", "thinking"];

/// What an engine offers to choose from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Offer {
    pub models: Vec<Choice>,
    pub efforts: Vec<Choice>,
    /// What a new session starts on.
    pub model: String,
    pub effort: String,
}

impl Offer {
    /// Read a `session/new` answer: config options first, then the older
    /// `models` list.
    pub fn read(created: &Value) -> Self {
        let options = created
            .get("configOptions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let find = |ids: &[&str]| {
            options.iter().find(|o| {
                o.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| ids.contains(&id))
            })
        };
        let current = |o: Option<&Value>| {
            o.and_then(|o| o.get("currentValue"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let model = find(&["model"]);
        let effort = find(&EFFORT_IDS);
        let mut offer = Offer {
            models: model.map(choices).unwrap_or_default(),
            efforts: effort.map(choices).unwrap_or_default(),
            model: current(model),
            effort: current(effort),
        };
        if offer.models.is_empty() {
            offer.models = created
                .pointer("/models/availableModels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|m| {
                    let value = m.get("modelId").and_then(Value::as_str)?;
                    Some(Choice {
                        value: value.to_owned(),
                        name: m
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or(value)
                            .to_owned(),
                    })
                })
                .collect();
            offer.model = created
                .pointer("/models/currentModelId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
        }
        offer
    }

    /// The name of `value` among the models, or the value itself.
    pub fn model_name(&self, value: &str) -> String {
        self.models
            .iter()
            .find(|c| c.value == value)
            .map_or_else(|| value.to_owned(), |c| c.name.clone())
    }
}
