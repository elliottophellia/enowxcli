//! Atomic JSONL session snapshots, including tool calls and results for replay.

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::sessions_dir;
use crate::message::{Message, Role as MessageRole};
use crate::role::Role;

/// One persisted transcript entry. Tool calls and results are kept so a resumed
/// session replays with the same context the model originally saw.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTurn {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub message: Message,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub workspace: PathBuf,
    /// Token accounting for the conversation so far. Persisted so resuming a
    /// session restores its context gauge and cost readout instead of showing
    /// zeros next to a transcript that plainly used tokens.
    #[serde(default)]
    pub usage: SessionUsage,
    /// Which agent currently holds the session. Empty on sessions written
    /// before agents existed; `Session::agent_or_default` resolves those from
    /// the legacy `role` so an old file still opens with something sensible.
    #[serde(default)]
    pub agent: String,
    /// Every handover this session has seen, oldest first. The transcript
    /// needs it: a reply that silently changes voice reads as the model
    /// behaving oddly rather than as a different agent answering.
    #[serde(default)]
    pub switches: Vec<AgentSwitch>,
    /// Set on a branch session: the session that delegated to it. A branch
    /// keeps its own transcript so it can be inspected, while its usage rolls
    /// up into the parent — without that the cost readout reports the router's
    /// spend alone and is wrong by an order of magnitude.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Every delegation this session made, oldest first. Its own turns carry
    /// only the sub-agent's report; this is how a resumed session reaches the
    /// branch holding the work behind it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub delegations: Vec<DelegationRecord>,
    pub turns: Vec<StoredTurn>,
}

/// One delegation, recorded on the session that made it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DelegationRecord {
    pub agent: String,
    /// The branch session the sub-agent worked in.
    pub session_id: String,
    /// Set once it has finished, when it ended in failure.
    #[serde(default)]
    pub failed: bool,
}

/// The reason recorded when the user picks an agent with `/agent`. The
/// orchestrator only takes back what it handed over, never the user's choice.
pub const USER_SWITCH_REASON: &str = "switched by the user";

/// The reason recorded when a specialist's turn ends and the conversation
/// goes back to the orchestrator.
pub const RETURN_REASON: &str = "finished";

/// One handover, recorded so the transcript can show who answered what.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSwitch {
    pub from: String,
    pub to: String,
    /// Why the router moved the work. Shown to the user; a switch with no
    /// stated reason is indistinguishable from a glitch.
    #[serde(default)]
    pub reason: String,
    /// How many turns had been recorded when this happened, so the UI can
    /// place the marker in the right spot without a timestamp search.
    pub at_turn: usize,
}

/// Running totals for one session.
///
/// `context_tokens` is the last call's prompt size — what the context gauge
/// shows — while the token counts accumulate across the whole session, which
/// is what the cost readout is derived from.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionUsage {
    #[serde(default)]
    pub input_tokens: u32,
    #[serde(default)]
    pub output_tokens: u32,
    #[serde(default)]
    pub context_tokens: u32,
}

impl Session {
    /// Which agent holds this session.
    ///
    /// Sessions written before agents existed have an empty `agent` and only
    /// the legacy `role`. Mapping one to the other here means an old session
    /// opens with a sensible agent instead of an empty system prompt, without
    /// every caller having to know about the migration.
    pub fn agent_or_default(&self) -> String {
        if !self.agent.trim().is_empty() {
            return crate::agent_def::canonical_name(&self.agent).to_owned();
        }
        match self.role {
            Role::Orchestrator => crate::agent_def::ORCHESTRATOR,
            Role::Writer => "docs",
            Role::Researcher => "research",
        }
        .to_owned()
    }

    /// Whether the orchestrator lent this conversation to the specialist now
    /// holding it, so it goes back when the specialist's turn ends.
    ///
    /// Only the user's own session (a delegated branch reports instead), and
    /// only when the last handover was the orchestrator's: a specialist the
    /// user chose with `/agent` keeps the conversation.
    pub fn lent_by_orchestrator(&self) -> bool {
        let holder = self.agent_or_default();
        self.parent.is_none()
            && holder != crate::agent_def::ORCHESTRATOR
            && self.switches.last().is_some_and(|switch| {
                switch.from == crate::agent_def::ORCHESTRATOR
                    && switch.to == holder
                    && switch.reason != USER_SWITCH_REASON
            })
    }

    /// Record a handover. Does nothing when the agent is unchanged, so a
    /// re-selection of the current agent does not litter the transcript.
    pub fn switch_agent(&mut self, to: impl Into<String>, reason: impl Into<String>) {
        let to = to.into();
        let from = self.agent_or_default();
        if from == to {
            return;
        }
        self.switches.push(AgentSwitch {
            from,
            to: to.clone(),
            reason: reason.into(),
            at_turn: self.turns.len(),
        });
        self.agent = to;
    }

    /// A session branched off this one for a sub-agent to work in.
    ///
    /// It carries its own transcript so the work can be inspected, and its
    /// `parent` so usage can roll up — a branch whose cost is not counted
    /// makes the readout wrong by an order of magnitude, not merely partial.
    pub fn branch(&self, agent: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: String::new(),
            role: self.role,
            created_at: now,
            updated_at: now,
            workspace: self.workspace.clone(),
            usage: SessionUsage::default(),
            agent: agent.into(),
            switches: Vec::new(),
            parent: Some(self.id.clone()),
            delegations: Vec::new(),
            turns: Vec::new(),
        }
    }

    /// Fold a finished branch's token usage into this session.
    pub fn absorb_usage(&mut self, branch: &SessionUsage) {
        self.usage.input_tokens = self.usage.input_tokens.saturating_add(branch.input_tokens);
        self.usage.output_tokens = self
            .usage
            .output_tokens
            .saturating_add(branch.output_tokens);
        // `context_tokens` is "how full is the window now" for THIS session, so
        // a branch's figure is not added to it.
    }

    pub fn new(role: Role) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: String::new(),
            role,
            created_at: now,
            updated_at: now,
            workspace: PathBuf::new(),
            usage: SessionUsage::default(),
            agent: String::new(),
            switches: Vec::new(),
            parent: None,
            delegations: Vec::new(),
            turns: Vec::new(),
        }
    }

    pub fn meta(&self) -> SessionMeta {
        SessionMeta {
            id: self.id.clone(),
            title: self.title.clone(),
            role: self.role,
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.turns.len(),
        }
    }

    pub fn push(&mut self, message: Message) {
        if self.title.is_empty() && message.role == MessageRole::User {
            self.title = derive_title(&message.content);
        }
        self.updated_at = Utc::now();
        self.turns.push(StoredTurn {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: self.updated_at,
            message,
        });
    }

    /// Index of the `n`th user turn, counting from zero. The transcript shows
    /// user messages as the points a conversation can be rewound to, so this
    /// is how the interface names one.
    pub fn user_turn_index(&self, nth: usize) -> Option<usize> {
        self.turns
            .iter()
            .enumerate()
            .filter(|(_, turn)| turn.message.role == MessageRole::User)
            .map(|(index, _)| index)
            .nth(nth)
    }

    /// Drop the turn at `index` and everything after it, returning what was
    /// removed. Editing or resending a message from the middle of a session
    /// makes every later reply an answer to a question that is no longer
    /// there, so the tail goes with it.
    ///
    /// The caller pushes the replacement message afterwards; this only
    /// rewinds. Returns an empty vector when `index` is past the end.
    pub fn truncate_from(&mut self, index: usize) -> Vec<StoredTurn> {
        if index >= self.turns.len() {
            return Vec::new();
        }
        let removed = self.turns.split_off(index);
        self.updated_at = Utc::now();
        // Switches recorded against a turn that no longer exists would place
        // handover markers past the end of the transcript.
        self.switches.retain(|switch| switch.at_turn <= index);
        // The title comes from the first user message. Rewinding past it
        // means the next one names the session instead.
        if !self
            .turns
            .iter()
            .any(|turn| turn.message.role == MessageRole::User)
        {
            self.title.clear();
        }
        removed
    }

    /// Repair calls left without results by a process exit. Never rerun a
    /// possibly completed side effect automatically when resuming.
    pub fn replay(&self) -> Vec<Message> {
        let mut messages = Vec::new();
        let mut pending: Vec<String> = Vec::new();
        for turn in &self.turns {
            let message = &turn.message;
            if message.interrupted {
                continue;
            }
            if message.role == MessageRole::Tool {
                if let Some(index) = pending
                    .iter()
                    .position(|id| Some(id) == message.tool_call_id.as_ref())
                {
                    pending.remove(index);
                    messages.push(message.clone());
                }
                continue;
            }
            for id in pending.drain(..) {
                messages.push(Message::tool_result(
                    id,
                    "Result unavailable after interruption; inspect state before retrying.",
                ));
            }
            pending.extend(message.tool_calls.iter().map(|call| call.id.clone()));
            messages.push(message.clone());
        }
        for id in pending {
            messages.push(Message::tool_result(
                id,
                "Result unavailable after interruption; inspect state before retrying.",
            ));
        }
        messages
    }
}

/// First line of the prompt, trimmed to something a sidebar can show.
fn derive_title(prompt: &str) -> String {
    let first = prompt
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut title: String = first.chars().take(60).collect();
    if first.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() {
        "New conversation".to_string()
    } else {
        title
    }
}

/// JSONL-backed store. The header line holds metadata; every later line is a turn.
#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Header {
    id: String,
    title: String,
    role: Role,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    workspace: PathBuf,
    /// Absent in files written before usage was persisted; `default` keeps
    /// those loadable, just with zeroed counters.
    #[serde(default)]
    usage: SessionUsage,
    /// Absent before agents existed; an old session resolves its agent from
    /// the legacy `role` instead.
    #[serde(default)]
    agent: String,
    #[serde(default)]
    switches: Vec<AgentSwitch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    delegations: Vec<DelegationRecord>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(sessions_dir())
    }
}

impl SessionStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, id: &str) -> Result<PathBuf> {
        uuid::Uuid::parse_str(id).context("invalid session id")?;
        Ok(self.root.join(format!("{id}.jsonl")))
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        std::fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let mut out = String::new();
        let header = Header {
            id: session.id.clone(),
            title: session.title.clone(),
            role: session.role,
            created_at: session.created_at,
            updated_at: session.updated_at,
            workspace: session.workspace.clone(),
            usage: session.usage,
            agent: session.agent.clone(),
            switches: session.switches.clone(),
            parent: session.parent.clone(),
            delegations: session.delegations.clone(),
        };
        out.push_str(&serde_json::to_string(&header)?);
        out.push('\n');
        for turn in &session.turns {
            out.push_str(&serde_json::to_string(turn)?);
            out.push('\n');
        }
        let path = self.path_for(&session.id)?;
        crate::config::atomic_write(&path, out.as_bytes())?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Session> {
        let path = self.path_for(id)?;
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut lines = text.lines();
        let header: Header = lines
            .next()
            .map(serde_json::from_str)
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("session {id} is empty"))?;
        let mut turns = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            turns.push(serde_json::from_str(line)?);
        }
        // Names recorded before an agent was renamed read as its name now,
        // so an old session's handovers and holder match the roster.
        let rename = |name: String| crate::agent_def::canonical_name(&name).to_owned();
        let switches = header
            .switches
            .into_iter()
            .map(|switch| AgentSwitch {
                from: rename(switch.from),
                to: rename(switch.to),
                ..switch
            })
            .collect();
        Ok(Session {
            id: header.id,
            title: header.title,
            role: header.role,
            created_at: header.created_at,
            updated_at: header.updated_at,
            workspace: header.workspace,
            usage: header.usage,
            agent: rename(header.agent),
            switches,
            parent: header.parent,
            delegations: header.delegations,
            turns,
        })
    }

    /// Newest first; malformed files are reported instead of silently hidden.
    /// The conversations, newest first: sessions the user talked in, not
    /// the transcripts of delegations.
    pub fn list(&self, limit: usize) -> Result<Vec<SessionMeta>> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(err) => return Err(err).context("listing sessions"),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let text = std::fs::read_to_string(&path)?;
            let mut lines = text.lines();
            let header: Header = serde_json::from_str(lines.next().unwrap_or(""))
                .with_context(|| format!("parsing {}", path.display()))?;
            // A delegation's transcript belongs to the conversation that
            // started it, which `branches_of` reaches; it is not one to
            // resume on its own.
            if header.parent.is_some() {
                continue;
            }
            let message_count = lines.filter(|line| !line.trim().is_empty()).count();
            out.push(SessionMeta {
                id: header.id,
                title: header.title,
                role: header.role,
                created_at: header.created_at,
                updated_at: header.updated_at,
                message_count,
            });
        }
        out.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
        out.truncate(limit);
        Ok(out)
    }

    /// The branches a session delegated to, oldest first, as delegation
    /// records. For sessions written before those were kept: a branch has
    /// always named its parent. Reads only each file's header line.
    pub fn branches_of(&self, parent: &str) -> Vec<DelegationRecord> {
        use std::io::BufRead as _;
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut found: Vec<(DateTime<Utc>, DelegationRecord)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let mut first = String::new();
            if std::io::BufReader::new(file).read_line(&mut first).is_err() {
                continue;
            }
            let Ok(header) = serde_json::from_str::<Header>(&first) else {
                continue;
            };
            if header.parent.as_deref() == Some(parent) {
                found.push((
                    header.created_at,
                    DelegationRecord {
                        agent: header.agent,
                        session_id: header.id,
                        failed: false,
                    },
                ));
            }
        }
        found.sort_by_key(|(created, _)| *created);
        found.into_iter().map(|(_, record)| record).collect()
    }

    /// Where `id`'s transcript is kept, when it is a session id.
    pub fn file_of(&self, id: &str) -> Option<PathBuf> {
        self.path_for(id).ok()
    }

    /// Every session `id` delegated to, theirs too, at any depth, with `id`
    /// first.
    pub fn family(&self, id: &str) -> Vec<String> {
        let mut out = vec![id.to_owned()];
        let mut at = 0;
        while at < out.len() && out.len() < 10_000 {
            for branch in self.branches_of(&out[at]) {
                if !out.contains(&branch.session_id) {
                    out.push(branch.session_id);
                }
            }
            at += 1;
        }
        out
    }

    /// Delete a session and every delegation under it. Returns how many
    /// transcripts were removed.
    pub fn delete_family(&self, id: &str) -> Result<usize> {
        let family = self.family(id);
        for member in &family {
            self.delete(member)?;
        }
        Ok(family.len())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path_for(id)?;
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err).with_context(|| format!("removing {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (SessionStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("enx-sessions-{}", uuid::Uuid::new_v4()));
        (SessionStore::new(dir.clone()), dir)
    }

    /// `/resume` lists conversations, not the transcripts of delegations.
    #[test]
    fn delegations_are_not_listed_as_conversations() {
        let (store, dir) = temp_store();
        let parent = Session::new(Role::Orchestrator);
        let child = parent.branch("fe");
        store.save(&parent).unwrap();
        store.save(&child).unwrap();
        let listed: Vec<String> = store.list(10).unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(listed, vec![parent.id.clone()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Deleting a session takes its delegations with it, at any depth, and
    /// nothing else.
    #[test]
    fn a_session_is_deleted_with_its_delegations() {
        let (store, dir) = temp_store();
        let parent = Session::new(Role::Orchestrator);
        let other = Session::new(Role::Orchestrator);
        let child = parent.branch("fe");
        let grandchild = child.branch("test");
        for session in [&parent, &other, &child, &grandchild] {
            store.save(session).unwrap();
        }
        assert_eq!(store.family(&parent.id).len(), 3);
        assert_eq!(store.delete_family(&parent.id).unwrap(), 3);
        assert!(store.load(&parent.id).is_err());
        assert!(store.load(&grandchild.id).is_err());
        assert!(
            store.load(&other.id).is_ok(),
            "another session is untouched"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_session_finds_its_branches_oldest_first() {
        let (store, dir) = temp_store();
        let parent = Session::new(Role::Orchestrator);
        store.save(&parent).unwrap();
        let mut first = parent.branch("fe");
        first.created_at = parent.created_at + chrono::Duration::seconds(1);
        let mut second = parent.branch("review");
        second.created_at = parent.created_at + chrono::Duration::seconds(2);
        store.save(&second).unwrap();
        store.save(&first).unwrap();
        store
            .save(&Session::new(Role::Orchestrator).branch("be"))
            .unwrap();

        let found = store.branches_of(&parent.id);
        let _ = std::fs::remove_dir_all(dir);
        let agents: Vec<&str> = found.iter().map(|r| r.agent.as_str()).collect();
        assert_eq!(agents, vec!["fe", "review"]);
        assert_eq!(found[0].session_id, first.id);
    }

    #[test]
    fn round_trip_preserves_tool_calls_and_title() {
        let (store, dir) = temp_store();
        let mut session = Session::new(Role::Researcher);
        session.push(Message::user("Where is the config parsed?\nsecond line"));
        session.push(Message {
            role: MessageRole::Assistant,
            content: String::new(),
            reasoning: None,
            tool_calls: vec![crate::message::ToolCall {
                id: "call_1".into(),
                name: "grep".into(),
                arguments: "{}".into(),
            }],
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        });
        session.push(Message::tool_result("call_1", "hit"));
        store.save(&session).unwrap();

        let loaded = store.load(&session.id).unwrap();
        assert_eq!(loaded.role, Role::Researcher);
        assert_eq!(loaded.title, "Where is the config parsed?");
        assert_eq!(loaded.turns.len(), 3);
        assert_eq!(loaded.turns[1].message.tool_calls[0].name, "grep");
        assert_eq!(
            loaded.turns[2].message.tool_call_id.as_deref(),
            Some("call_1")
        );

        let listed = store.list(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].message_count, 3);

        store.delete(&session.id).unwrap();
        assert!(store.list(10).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A crash between a tool call and its result leaves the transcript unpaired.
    /// Replay must synthesize the missing result rather than send an assistant
    /// tool call the provider will reject, and never silently rerun the tool.
    #[test]
    fn replay_pairs_calls_left_without_results() {
        let mut session = Session::new(Role::Orchestrator);
        session.push(Message::user("write the file"));
        session.push(Message {
            role: MessageRole::Assistant,
            content: String::new(),
            reasoning: None,
            tool_calls: vec![crate::message::ToolCall {
                id: "call_1".into(),
                name: "write".into(),
                arguments: "{}".into(),
            }],
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: false,
            error: None,
            model: None,
            message_id: None,
        });
        session.push(Message {
            role: MessageRole::Assistant,
            content: "partial".into(),
            reasoning: None,
            tool_calls: Vec::new(),
            attachments: Vec::new(),
            tool_call_id: None,
            interrupted: true,
            error: Some("Interrupted by user.".into()),
            model: None,
            message_id: None,
        });

        let replay = session.replay();
        // The interrupted partial reply stays out of provider context.
        assert_eq!(replay.len(), 3);
        assert_eq!(replay[2].role, MessageRole::Tool);
        assert_eq!(replay[2].tool_call_id.as_deref(), Some("call_1"));
        assert!(replay[2].content.contains("Result unavailable"));
        // The partial text is still visible in stored history for the user.
        assert_eq!(session.turns.len(), 3);
    }
}

#[cfg(test)]
mod usage_persistence_tests {
    use super::*;

    fn store() -> (SessionStore, tempdir::Guard) {
        let guard = tempdir::Guard::new();
        (SessionStore::new(guard.path().to_path_buf()), guard)
    }

    #[test]
    fn usage_survives_a_save_and_load() {
        let (store, _guard) = store();
        let mut session = Session::new(Role::Orchestrator);
        session.usage = SessionUsage {
            input_tokens: 12_345,
            output_tokens: 678,
            context_tokens: 9_012,
        };
        store.save(&session).expect("save");
        let loaded = store.load(&session.id).expect("load");
        assert_eq!(
            loaded.usage, session.usage,
            "the counters shown beside a resumed transcript come from here"
        );
    }

    /// Files written before usage was persisted must still load, with zeroed
    /// counters rather than an error — every existing session is one of these.
    #[test]
    fn a_session_without_usage_still_loads() {
        let (store, guard) = store();
        let id = "11111111-2222-3333-4444-555555555555";
        let header = format!(
            r#"{{"id":"{id}","title":"old","role":"orchestrator","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","workspace":"/tmp"}}"#
        );
        std::fs::write(guard.path().join(format!("{id}.jsonl")), header + "\n").unwrap();
        let loaded = store.load(id).expect("a pre-usage session must still open");
        assert_eq!(loaded.usage, SessionUsage::default());
        assert_eq!(loaded.title, "old");
    }

    /// The context gauge shows how full the window is now, so it tracks the
    /// last call rather than accumulating.
    #[test]
    fn context_is_the_latest_prompt_while_totals_accumulate() {
        let mut usage = SessionUsage::default();
        for prompt in [1_000u32, 2_500, 4_000] {
            usage.input_tokens = usage.input_tokens.saturating_add(prompt);
            usage.output_tokens = usage.output_tokens.saturating_add(100);
            usage.context_tokens = prompt;
        }
        assert_eq!(usage.input_tokens, 7_500, "input accumulates");
        assert_eq!(usage.output_tokens, 300, "output accumulates");
        assert_eq!(usage.context_tokens, 4_000, "context is the last prompt");
    }

    pub(super) mod tempdir {
        pub struct Guard(std::path::PathBuf);
        impl Guard {
            pub fn new() -> Self {
                // A timestamp alone collides when tests start in the same
                // nanosecond under the parallel runner; the counter makes the
                // name unique within the process.
                use std::sync::atomic::{AtomicU64, Ordering};
                static SEQ: AtomicU64 = AtomicU64::new(0);
                let path = std::env::temp_dir().join(format!(
                    "enx-session-test-{}-{}",
                    std::process::id(),
                    SEQ.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).unwrap();
                Self(path)
            }
            pub fn path(&self) -> &std::path::Path {
                &self.0
            }
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }
}

#[cfg(test)]
mod agent_tests {
    use super::*;

    #[test]
    fn a_new_session_has_no_agent_until_one_is_set() {
        let s = Session::new(Role::Orchestrator);
        assert!(s.agent.is_empty());
        assert!(s.switches.is_empty());
        assert!(s.parent.is_none());
    }

    /// Sessions written before agents existed carry only the legacy role.
    /// Resolving them here means an old file opens with a sensible agent
    /// rather than an empty system prompt.
    #[test]
    fn a_legacy_session_resolves_its_agent_from_the_role() {
        for (role, expected) in [
            (Role::Orchestrator, crate::agent_def::ORCHESTRATOR),
            (Role::Writer, "docs"),
            (Role::Researcher, "research"),
        ] {
            let s = Session::new(role);
            assert_eq!(s.agent_or_default(), expected);
        }
    }

    #[test]
    fn an_explicit_agent_wins_over_the_legacy_role() {
        let mut s = Session::new(Role::Researcher);
        s.agent = "fe".into();
        assert_eq!(s.agent_or_default(), "fe");
    }

    #[test]
    fn a_switch_is_recorded_with_its_reason_and_position() {
        let mut s = Session::new(Role::Orchestrator);
        s.turns.push(StoredTurn {
            id: "1".into(),
            created_at: Utc::now(),
            message: Message::user("hi"),
        });
        s.switch_agent("fe", "the request is about the login page");

        assert_eq!(s.agent, "fe");
        assert_eq!(s.switches.len(), 1);
        let sw = &s.switches[0];
        assert_eq!(sw.from, crate::agent_def::ORCHESTRATOR);
        assert_eq!(sw.to, "fe");
        assert_eq!(
            sw.at_turn, 1,
            "the marker belongs after the turn it follows"
        );
        assert!(
            !sw.reason.is_empty(),
            "a switch with no reason reads as a glitch"
        );
    }

    /// Re-selecting the current agent is a no-op; otherwise the transcript
    /// fills with markers for handovers that did not happen.
    #[test]
    fn switching_to_the_same_agent_records_nothing() {
        let mut s = Session::new(Role::Orchestrator);
        s.agent = "fe".into();
        s.switch_agent("fe", "again");
        assert!(s.switches.is_empty());
    }

    /// A session saved while the orchestrator was called `router` loads with
    /// the current name, holder and handovers alike.
    #[test]
    fn an_old_session_loads_under_the_current_names() {
        let dir = std::env::temp_dir().join(format!("enx-sessions-{}", uuid::Uuid::new_v4()));
        let store = SessionStore::new(dir.clone());
        let mut s = Session::new(Role::Orchestrator);
        s.agent = "router".into();
        s.switches.push(AgentSwitch {
            from: "router".into(),
            to: "fe".into(),
            reason: "frontend work".into(),
            at_turn: 0,
        });
        store.save(&s).unwrap();
        let loaded = store.load(&s.id).unwrap();
        let _ = std::fs::remove_dir_all(dir);
        assert_eq!(loaded.agent, crate::agent_def::ORCHESTRATOR);
        assert_eq!(loaded.agent_or_default(), crate::agent_def::ORCHESTRATOR);
        assert_eq!(loaded.switches[0].from, crate::agent_def::ORCHESTRATOR);
        assert_eq!(loaded.switches[0].to, "fe");
    }

    #[test]
    fn switches_accumulate_in_order() {
        let mut s = Session::new(Role::Orchestrator);
        s.switch_agent("fe", "ui work");
        s.switch_agent("be", "now the api");
        let path: Vec<&str> = s.switches.iter().map(|x| x.to.as_str()).collect();
        assert_eq!(path, vec!["fe", "be"]);
        assert_eq!(s.switches[1].from, "fe");
    }

    #[test]
    fn a_branch_starts_clean_but_remembers_its_parent() {
        let mut parent = Session::new(Role::Orchestrator);
        parent.agent = crate::agent_def::ORCHESTRATOR.into();
        parent.turns.push(StoredTurn {
            id: "1".into(),
            created_at: Utc::now(),
            message: Message::user("do a thing"),
        });
        parent.usage.input_tokens = 500;

        let branch = parent.branch("fe");
        assert_eq!(branch.parent.as_deref(), Some(parent.id.as_str()));
        assert_eq!(branch.agent, "fe");
        assert!(
            branch.turns.is_empty(),
            "a sub-agent starts from a briefing"
        );
        assert_eq!(
            branch.usage,
            SessionUsage::default(),
            "the branch accounts for its own spend"
        );
        assert_ne!(branch.id, parent.id);
        assert_eq!(branch.workspace, parent.workspace);
    }

    /// Without the roll-up the sidebar reports the router's spend alone, which
    /// is a wrong number rather than an incomplete one.
    #[test]
    fn branch_usage_rolls_up_into_the_parent() {
        let mut parent = Session::new(Role::Orchestrator);
        parent.usage = SessionUsage {
            input_tokens: 1_000,
            output_tokens: 200,
            context_tokens: 1_000,
        };
        let branch_usage = SessionUsage {
            input_tokens: 28_000,
            output_tokens: 3_000,
            context_tokens: 28_000,
        };
        parent.absorb_usage(&branch_usage);

        assert_eq!(parent.usage.input_tokens, 29_000);
        assert_eq!(parent.usage.output_tokens, 3_200);
        assert_eq!(
            parent.usage.context_tokens, 1_000,
            "context is how full THIS window is; a branch does not fill it"
        );
    }

    #[test]
    fn the_new_fields_survive_a_save_and_load() {
        let guard = super::usage_persistence_tests::tempdir::Guard::new();
        let store = SessionStore::new(guard.path().to_path_buf());
        let mut s = Session::new(Role::Orchestrator);
        s.switch_agent("fe", "ui work");
        s.parent = Some("parent-id".into());
        store.save(&s).expect("save");

        let loaded = store.load(&s.id).expect("load");
        assert_eq!(loaded.agent, "fe");
        assert_eq!(loaded.switches.len(), 1);
        assert_eq!(loaded.switches[0].reason, "ui work");
        assert_eq!(loaded.parent.as_deref(), Some("parent-id"));
    }

    /// Lent: the orchestrator's handover. Kept: the user's pick, anything in
    /// a delegated branch, and the orchestrator itself.
    #[test]
    fn only_the_orchestrators_handover_is_lent() {
        let mut handed = Session::new(Role::Orchestrator);
        handed.switch_agent("fe", "frontend work");
        assert!(handed.lent_by_orchestrator());

        let mut picked = Session::new(Role::Orchestrator);
        picked.switch_agent("fe", USER_SWITCH_REASON);
        assert!(!picked.lent_by_orchestrator(), "the user's pick stays");

        let mut returned = handed.clone();
        returned.switch_agent(crate::agent_def::ORCHESTRATOR, RETURN_REASON);
        assert!(!returned.lent_by_orchestrator(), "already back");

        let mut branch = handed.clone();
        branch.parent = Some("parent-id".into());
        assert!(!branch.lent_by_orchestrator(), "a branch reports instead");

        assert!(!Session::new(Role::Orchestrator).lent_by_orchestrator());
    }

    /// Every session file on disk today predates these fields.
    #[test]
    fn a_session_file_without_agent_fields_still_loads() {
        let guard = super::usage_persistence_tests::tempdir::Guard::new();
        let store = SessionStore::new(guard.path().to_path_buf());
        let id = "22222222-3333-4444-5555-666666666666";
        let header = format!(
            r#"{{"id":"{id}","title":"old","role":"writer","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z","workspace":"/tmp"}}"#
        );
        std::fs::write(guard.path().join(format!("{id}.jsonl")), header + "\n").unwrap();

        let loaded = store.load(id).expect("a pre-agent session must still open");
        assert!(loaded.agent.is_empty());
        assert_eq!(loaded.agent_or_default(), "docs", "resolved from the role");
        assert!(loaded.switches.is_empty());
        assert!(loaded.parent.is_none());
    }
}

#[cfg(test)]
mod truncate_tests {
    use super::*;

    fn conversation() -> Session {
        let mut session = Session::new(Role::Orchestrator);
        session.push(Message::user("write the parser"));
        session.push(Message::assistant("here it is"));
        session.push(Message::user("now add tests"));
        session.push(Message::assistant("here are the tests"));
        session
    }

    #[test]
    fn it_drops_the_turn_and_everything_after_it() {
        let mut session = conversation();
        let removed = session.truncate_from(2);
        assert_eq!(removed.len(), 2, "the turn and the reply after it");
        assert_eq!(session.turns.len(), 2);
        assert_eq!(session.turns[1].message.content, "here it is");
    }

    /// The point of rewinding is that the model no longer sees what was cut.
    #[test]
    fn the_replay_no_longer_carries_the_removed_turns() {
        let mut session = conversation();
        session.truncate_from(2);
        let replayed = session.replay();
        assert!(
            !replayed.iter().any(|m| m.content.contains("add tests")),
            "the cut message should not reach the model: {replayed:?}"
        );
        assert_eq!(replayed.len(), 2);
    }

    #[test]
    fn it_finds_the_nth_user_turn() {
        let session = conversation();
        assert_eq!(session.user_turn_index(0), Some(0));
        assert_eq!(session.user_turn_index(1), Some(2));
        assert_eq!(session.user_turn_index(2), None);
    }

    /// A handover recorded against a cut turn would put a marker past the end
    /// of the transcript.
    #[test]
    fn switches_past_the_cut_are_dropped() {
        let mut session = conversation();
        session.switch_agent("fe", "this is a UI question");
        assert_eq!(session.switches.len(), 1);
        session.truncate_from(2);
        assert!(
            session.switches.iter().all(|s| s.at_turn <= 2),
            "a switch recorded at turn 4 should not survive a cut to 2: {:?}",
            session.switches
        );
    }

    /// Rewinding past the first message means the session has no question in
    /// it any more, so the next one should name it.
    #[test]
    fn cutting_everything_clears_the_title() {
        let mut session = conversation();
        assert!(!session.title.is_empty());
        session.truncate_from(0);
        assert!(session.turns.is_empty());
        assert!(
            session.title.is_empty(),
            "the next message should retitle it"
        );
    }

    #[test]
    fn an_index_past_the_end_changes_nothing() {
        let mut session = conversation();
        let removed = session.truncate_from(99);
        assert!(removed.is_empty());
        assert_eq!(session.turns.len(), 4);
    }
}
