//! `/handoff`: the conversation carries on in a fresh session, its history
//! folded into a summary, so the context is light again. The user chooses
//! whether the old session's history (its transcript and every delegation's)
//! is kept or deleted.

use super::*;

/// A handoff running off the UI thread.
pub(crate) struct Pending {
    pub old: String,
    pub delete: bool,
    pub result: tokio::sync::oneshot::Receiver<anyhow::Result<String>>,
}

impl App {
    /// `/handoff`: ask whether to keep this session's history.
    pub(crate) fn open_handoff(&mut self) -> Result<()> {
        // Said in the transcript, as `/compact` says it: none of these is an
        // error, only not yet.
        let not_yet = if self.session_id.is_none() {
            Some("nothing to hand off yet: send a message first")
        } else if self.busy {
            Some("a turn is running; wait for it or stop it first")
        } else if self.agent.delegations_running() > 0 {
            Some("delegations are still at work; hand off once they have reported")
        } else {
            None
        };
        if let Some(why) = not_yet {
            self.push(TranscriptKind::Notice, why);
            return Ok(());
        }
        let id = self.session_id.clone().expect("checked above");
        let (bytes, branches) = crate::resources::on_disk(self.agent.store(), &id);
        let size = crate::resources::human(bytes);
        self.modal_items = vec![
            (
                "keep".into(),
                "Keep this session's history. It stays in /resume.".into(),
            ),
            (
                "delete".into(),
                format!(
                    "Delete this session's history: its transcript and {branches} {}, {size} on disk.",
                    if branches == 1 { "delegation" } else { "delegations" }
                ),
            ),
            ("cancel".into(), "Stay in this session.".into()),
        ];
        self.modal_cursor = 0;
        self.modal = Modal::Handoff;
        Ok(())
    }

    /// Summarise into a new session off the UI thread; `tick_handoff` moves
    /// over when it is ready.
    pub(crate) fn start_handoff(&mut self, delete: bool) -> Result<()> {
        let Some(old) = self.session_id.clone() else {
            return Ok(());
        };
        let agent = self.agent.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        let id = old.clone();
        tokio::spawn(async move {
            let result = agent.handoff(&id).await.map(|session| session.id);
            let _ = tx.send(result);
        });
        self.handoff = Some(Pending {
            old,
            delete,
            result: rx,
        });
        self.busy = true;
        self.turn_started = Instant::now();
        self.status = "handing off: summarising this session…".into();
        Ok(())
    }

    /// Each frame: when the new session is ready, open it, and delete the
    /// old one's history if that was asked for.
    pub(crate) fn tick_handoff(&mut self) {
        let Some(pending) = self.handoff.as_mut() else {
            return;
        };
        let result = match pending.result.try_recv() {
            Ok(result) => result,
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => return,
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                Err(anyhow::anyhow!("the handoff stopped without a result"))
            }
        };
        let pending = self.handoff.take().expect("pending handoff");
        self.busy = false;
        match result {
            Err(error) => {
                self.push(TranscriptKind::Error, format!("handoff failed: {error:#}"));
                self.status = "handoff failed".into();
            }
            Ok(new) => {
                if let Err(error) = self.resume(&new) {
                    self.push(TranscriptKind::Error, format!("{error:#}"));
                    return;
                }
                let note = if pending.delete {
                    match self.agent.store().delete_family(&pending.old) {
                        Ok(count) => format!(
                            "Handed off to a new session; the old one's history was deleted ({count} {}).",
                            if count == 1 { "transcript" } else { "transcripts" }
                        ),
                        Err(error) => format!(
                            "Handed off to a new session, but deleting the old one failed: {error:#}"
                        ),
                    }
                } else {
                    "Handed off to a new session; the old one is kept in /resume.".to_owned()
                };
                self.push(TranscriptKind::Notice, note);
                self.status = "handed off".into();
            }
        }
    }

    /// Each frame: memory, CPU and disk, every few seconds.
    /// How long each running delegation has been silent. Reads file times
    /// only, every few seconds, so it never holds up a frame.
    pub(crate) fn tick_delegation_watch(&mut self) {
        /// Silence worth pointing out.
        const QUIET: u64 = 300;
        if self
            .quiet_checked
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(5))
        {
            return;
        }
        self.quiet_checked = Some(Instant::now());
        let store = self.agent.store().clone();
        let now = std::time::SystemTime::now();
        let mut quiet = HashMap::new();
        for delegation in &self.delegations {
            if delegation.state != crate::app::DelegationState::Running {
                continue;
            }
            let Some(written) = store.modified(&delegation.session_id) else {
                continue;
            };
            let secs = now.duration_since(written).map_or(0, |d| d.as_secs());
            let was = self
                .delegation_quiet
                .get(&delegation.session_id)
                .copied()
                .unwrap_or(0);
            if secs >= QUIET && was < QUIET {
                self.status = format!(
                    "{} has made no progress for {} min; right-click it in the sidebar to stop it",
                    enowx_core::agent_def::display_name(&delegation.agent),
                    secs / 60
                );
            }
            quiet.insert(delegation.session_id.clone(), secs);
        }
        self.delegation_quiet = quiet;
    }

    pub(crate) fn tick_resources(&mut self) {
        let store = self.agent.store().clone();
        let session = self.session_id.clone();
        self.resources.tick(&store, session.as_deref());
    }
}
