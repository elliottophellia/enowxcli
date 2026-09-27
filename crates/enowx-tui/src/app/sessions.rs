use super::*;

impl App {
    /// Open the modal picker for saved sessions in this workspace.
    pub(crate) fn open_sessions(&mut self) -> Result<()> {
        let workspace = std::fs::canonicalize(self.config.workspace())
            .unwrap_or_else(|_| self.config.workspace());
        let mut items = Vec::new();
        for meta in self.store.list(200)? {
            // A session belongs to the workspace it was created in. Loading
            // the header is cheap because `list` already parsed the file.
            let owned = self
                .store
                .load(&meta.id)
                .map(|session| session.workspace == workspace)
                .unwrap_or(false);
            if owned {
                items.push((
                    meta.id.clone(),
                    format!(
                        "{} · {} · {} messages",
                        if meta.title.is_empty() {
                            "Untitled".into()
                        } else {
                            meta.title
                        },
                        meta.role.label(),
                        meta.message_count
                    ),
                ));
            }
        }
        if items.is_empty() {
            self.push(
                TranscriptKind::System,
                "No saved sessions in this workspace.",
            );
            return Ok(());
        }
        self.modal_items = items;
        self.modal = Modal::Sessions;
        self.modal_cursor = 0;
        Ok(())
    }

    pub(crate) fn resume(&mut self, id: &str) -> Result<()> {
        let session = self.store.load(id)?;
        let workspace = std::fs::canonicalize(self.config.workspace())?;
        anyhow::ensure!(
            session.workspace == workspace,
            "Session belongs to {}",
            session.workspace.display()
        );
        // Restore the counters the session recorded rather than zeroing them:
        // the transcript comes back showing work that plainly consumed
        // tokens, so a 0/1,000,000 gauge beside it is simply wrong. Sessions
        // written before usage was persisted carry zeros, which is honest —
        // the numbers were never captured.
        self.tokens_in = session.usage.input_tokens;
        self.tokens_out = session.usage.output_tokens;
        self.context_tokens = session.usage.context_tokens;
        // Counted here, from the session being adopted, and not in the
        // replay: the replay also draws a sub-agent's branch for viewing, and
        // re-reads it every half second while it runs. Counting there added
        // the branch's calls to this session's total on every re-read — a
        // portfolio that took eight tool calls showed 1,175.
        self.tool_counts = count_tool_calls(&session);
        self.session_id = Some(session.id.clone());
        self.adopt_agent(&session);
        self.title = session.title.clone();
        self.role = session.role;
        self.blocks.clear();
        self.switch_markers.clear();
        self.events = None;
        self.auto_scroll = true;
        self.replay_into_blocks(&session);
        self.status = format!("resumed {}", &session.id[..8]);
        Ok(())
    }

    /// Turn a stored session into transcript blocks. Shared by `resume` and
    /// by viewing a delegation, so the two cannot drift apart.
    /// Replay writes what is SHOWN, so it goes to `self.blocks` directly
    /// rather than through `push`. `push` routes a turn's events to the main
    /// conversation, which is the opposite of what is wanted here: the whole
    /// point is to put a branch on screen.
    fn show(&mut self, kind: TranscriptKind, text: impl Into<String>) {
        self.blocks.push(crate::session::TranscriptBlock {
            kind,
            text: text.into(),
        });
    }

    pub(crate) fn replay_into_blocks(&mut self, session: &enowx_core::Session) {
        let mut tool_blocks: HashMap<String, usize> = HashMap::new();
        // A switch names the turn it happened at, but a turn expands into
        // several blocks, so the marker is pinned to the first block a turn
        // produces as the replay reaches it.
        let mut switches = session.switches.iter().peekable();
        for (turn_index, turn) in session.turns.iter().enumerate() {
            while switches
                .peek()
                .is_some_and(|switch| switch.at_turn <= turn_index)
            {
                let switch = switches.next().expect("peeked");
                self.switch_markers
                    .push((self.blocks.len(), switch.clone()));
            }
            let turn = turn.clone();
            match turn.message.role {
                MessageRole::User => {
                    let mut display = turn.message.content.clone();
                    if !turn.message.attachments.is_empty() {
                        let mut names: Vec<String> = turn
                            .message
                            .attachments
                            .iter()
                            .map(|a| format!("📎 {}", a.name))
                            .collect();
                        names.sort();
                        if !display.is_empty() {
                            display.push_str("\n\n");
                        }
                        display.push_str(&names.join("  "));
                    }
                    self.show(TranscriptKind::User, display);
                }
                MessageRole::Assistant => {
                    if let Some(reasoning) = turn.message.reasoning {
                        self.show(TranscriptKind::Reasoning, reasoning);
                    }
                    if !turn.message.content.is_empty() {
                        self.show(TranscriptKind::Assistant, turn.message.content);
                    }
                    for call in turn.message.tool_calls {
                        let index = self.blocks.len();
                        tool_blocks.insert(call.id.clone(), index);
                        self.show(
                            TranscriptKind::Tool {
                                id: call.id,
                                name: call.name,
                                args: call.arguments,
                                result: String::new(),
                                running: true,
                                error: false,
                                // Replayed from a session file: the original
                                // start time is not recorded, and timing a
                                // finished call from "now" would be wrong.
                                started: None,
                            },
                            String::new(),
                        );
                    }
                    if let Some(error) = turn.message.error {
                        self.show(TranscriptKind::Error, error);
                    }
                }
                MessageRole::Tool => {
                    if let Some(id) = turn.message.tool_call_id {
                        if let Some(index) = tool_blocks.get(&id).copied() {
                            if let TranscriptKind::Tool {
                                result,
                                running,
                                error,
                                ..
                            } = &mut self.blocks[index].kind
                            {
                                *result = turn.message.content;
                                *running = false;
                                *error = turn.message.error.is_some();
                            }
                        }
                    }
                }
                MessageRole::System => self.show(TranscriptKind::System, turn.message.content),
            }
        }
        // A handover recorded after the last turn — the usual case, since a
        // switch is written before the new agent has answered — still belongs
        // in the transcript, at the bottom.
        for switch in switches {
            self.switch_markers
                .push((self.blocks.len(), switch.clone()));
        }
    }
}

/// How many times each tool was called in one session's own turns.
fn count_tool_calls(session: &enowx_core::Session) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for turn in &session.turns {
        for call in &turn.message.tool_calls {
            *counts.entry(call.name.clone()).or_insert(0) += 1;
        }
    }
    counts
}

impl App {
    /// Open a delegation's branch session read-only.
    ///
    /// A sub-agent's work is real work — files written, commands run — and
    /// reporting only a one-line summary for it leaves the user with no way
    /// to check any of it. The branch is already on disk; this is the way in.
    ///
    /// Deliberately not `resume`: that adopts the session as the one being
    /// worked in. This keeps the main conversation as the live one and puts
    /// its blocks aside to restore on the way back.
    pub(crate) fn view_delegation(&mut self, index: usize) -> Result<()> {
        let Some(delegation) = self.delegations.get(index).cloned() else {
            return Ok(());
        };
        if self.viewing.is_some() {
            // Already looking at one: go back first so the saved main
            // conversation is never overwritten by another branch.
            self.leave_delegation();
        }
        let branch = self.store.load(&delegation.session_id)?;
        let blocks_at_open = self.blocks.len();
        let saved = crate::app::Viewing {
            blocks: std::mem::take(&mut self.blocks),
            scroll: self.scroll,
            auto_scroll: self.auto_scroll,
            agent: delegation.agent.clone(),
            index,
            session_id: delegation.session_id.clone(),
            last_refresh: std::time::Instant::now(),
            blocks_at_open,
        };
        self.render_cache.clear();
        self.switch_markers.clear();
        self.replay_into_blocks(&branch);
        self.viewing = Some(saved);
        self.auto_scroll = true;
        self.scroll = 0;
        self.status = format!("viewing {} · Esc to go back", delegation.agent);
        Ok(())
    }

    /// Re-read the branch on screen if the sub-agent is still writing to it.
    ///
    /// Without this the transcript is whatever the file held at the moment it
    /// was opened, so a sub-agent that is working looks exactly like one that
    /// has stopped — which is the question someone opens it to answer.
    ///
    /// Only while it runs, and not on every frame: a finished branch cannot
    /// change, and re-reading one at 25Hz would be pure waste.
    pub(crate) fn refresh_viewed_delegation(&mut self) {
        const EVERY: std::time::Duration = std::time::Duration::from_millis(500);
        let Some(viewing) = self.viewing.as_ref() else {
            return;
        };
        if self.delegations.get(viewing.index).map(|d| d.state)
            != Some(crate::app::DelegationState::Running)
        {
            return;
        }
        if viewing.last_refresh.elapsed() < EVERY {
            return;
        }
        let id = viewing.session_id.clone();
        let Ok(branch) = self.store.load(&id) else {
            return;
        };
        let before = self.blocks.len();
        // Rebuild rather than append: a tool call already on screen gains its
        // result in place, which appending cannot express.
        self.blocks.clear();
        self.switch_markers.clear();
        self.replay_into_blocks(&branch);
        if let Some(viewing) = self.viewing.as_mut() {
            viewing.last_refresh = std::time::Instant::now();
        }
        if self.blocks.len() != before {
            // Only invalidate when the shape changed; the cache is keyed on
            // content, so an unchanged block re-renders from it.
            self.render_cache.clear();
        }
    }

    /// Put the main conversation back.
    pub(crate) fn leave_delegation(&mut self) {
        let Some(saved) = self.viewing.take() else {
            return;
        };
        self.blocks = saved.blocks;
        self.scroll = saved.scroll;
        self.auto_scroll = saved.auto_scroll;
        self.render_cache.clear();
        self.switch_markers.clear();
        self.status = format!("back from {}", saved.agent);
    }
}
