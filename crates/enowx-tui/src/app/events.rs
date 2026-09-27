use super::*;

impl App {
    pub(crate) fn start_turn(&mut self, prompt: String) {
        if self.busy {
            self.status = "Still working; Ctrl+C interrupts".into();
            return;
        }
        // A sub-agent's transcript is a record, not a conversation to join:
        // its session is finished and the reply would land somewhere the user
        // is not looking. Go back first, then send.
        if self.viewing.is_some() {
            self.leave_delegation();
        }
        // Attachments already live as inline `[Image N]` chips inside the
        // prompt; the transcript replays the same string, and the payload sent
        // to the provider strips the chips so only real prose reaches the model.
        let attachments = std::mem::take(&mut self.attachments);
        self.attach_error = None;
        // Show attachments alongside the prose so the transcript reflects
        // what the model actually saw (and what a resumed session should
        // replay).
        let mut display = prompt.clone();
        if !attachments.is_empty() {
            let mut names: Vec<String> = attachments
                .iter()
                .map(|a| format!("📎 {}", a.name))
                .collect();
            names.sort();
            if !display.is_empty() {
                display.push_str("\n\n");
            }
            display.push_str(&names.join("  "));
        }
        self.push(TranscriptKind::User, display);
        let clean_prompt = crate::attachments::strip_chips(&prompt);
        self.auto_scroll = true;
        self.busy = true;
        self.turn_started = Instant::now();
        self.set_activity(Activity::Waiting);
        self.status = "working".into();
        // Any `Done` still owed by a stopped turn arrives on the channel that
        // is about to be replaced, so it can never reach us. Holding the debt
        // open would make this turn's own `Done` pay it off instead, leaving
        // the app busy until the channel closed — reported to the user as
        // "Agent stopped without a terminal event".
        self.forget_abandoned_turns();
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(256);
        self.cancel = Some(cancel.clone());
        self.events = Some(rx);
        let agent = self.agent.clone();
        let request = RunRequest {
            session_id: self.session_id.clone(),
            prompt: clean_prompt,
            role: self.role,
            attachments,
            agent: self.agent_for_new_session(),
        };
        self.task = Some(tokio::spawn(async move {
            // Report the failure rather than dropping it. Without this the
            // channel simply closes, and the only thing the user is told is
            // that the agent "stopped without a terminal event" — which says
            // nothing about what actually went wrong.
            if let Err(error) = agent.run(request, tx.clone(), cancel).await {
                let _ = tx
                    .send(Event::Error {
                        message: format!("{error:#}"),
                    })
                    .await;
            }
        }));
    }

    /// The agent a message starts a new session with. One picked with
    /// `/agent` before the first message had no session to be saved in, and
    /// without this the session the message creates began with the
    /// orchestrator while the status bar named the pick.
    pub(crate) fn agent_for_new_session(&self) -> Option<String> {
        self.session_id
            .is_none()
            .then(|| self.agent_name.clone())
            .filter(|name| name != enowx_core::agent_def::ORCHESTRATOR)
    }

    /// A `Done` still owed by a stopped turn arrives on the channel that
    /// starting a new turn replaces, so it can never be delivered. Clearing
    /// the debt stops this turn's own `Done` from paying it off instead.
    pub(crate) fn forget_abandoned_turns(&mut self) {
        self.abandoned = 0;
    }

    pub(crate) fn interrupt(&mut self) {
        // Stopping the turn is also the way out of a question: the agent
        // is told the user stopped instead of answering.
        self.question = None;
        self.question_rows.clear();
        let Some(cancel) = self.cancel.take() else {
            self.status = "nothing running".into();
            return;
        };
        cancel.cancel();
        // Stop being busy now rather than when the backend finishes tidying
        // up. It still has a partial reply to persist and a task to wind
        // down, and waiting for its `Done` left the composer locked and the
        // spinner turning for as long as that took — which reads as the key
        // not having worked.
        self.busy = false;
        self.set_activity(Activity::Idle);
        self.status = "stopped".into();
        // The task owns the channel the events arrive on. Dropping the
        // receiver here would discard the events it still has to send, so it
        // is left in place: `Done` for the abandoned turn is ignored below.
        self.abandoned += 1;
    }

    pub(crate) fn apply_event(&mut self, event: Event) {
        if !matches!(event, Event::Reasoning { .. }) {
            self.finish_thinking();
        }
        match event {
            Event::Session { id, title } => {
                self.session_id = Some(id);
                self.title = title;
            }
            Event::MessageStart { .. } => self.set_activity(Activity::Waiting),
            Event::Text { delta } => {
                self.set_activity(Activity::Writing);
                if let Some(last) = self
                    .conversation_mut()
                    .last_mut()
                    .filter(|block| matches!(block.kind, TranscriptKind::Assistant))
                {
                    last.text.push_str(&delta);
                } else {
                    self.push(TranscriptKind::Assistant, delta);
                }
            }
            Event::Reasoning { delta } => {
                self.set_activity(Activity::Thinking);
                if let Some(last) = self.conversation_mut().last_mut().filter(|block| {
                    matches!(
                        block.kind,
                        TranscriptKind::Reasoning {
                            started: Some(_),
                            elapsed: None,
                            ..
                        }
                    )
                }) {
                    last.text.push_str(&delta);
                } else {
                    self.reasoning_seq += 1;
                    let id = format!("thinking-{}", self.reasoning_seq);
                    self.push(
                        TranscriptKind::Reasoning {
                            id,
                            started: Some(std::time::Instant::now()),
                            elapsed: None,
                        },
                        delta,
                    );
                }
            }
            Event::ToolCall {
                id,
                name,
                arguments,
            } => {
                *self.tool_counts.entry(name.clone()).or_insert(0) += 1;
                self.set_activity(Activity::Tool(name.clone()));
                self.push(
                    TranscriptKind::Tool {
                        id,
                        name,
                        args: arguments,
                        result: String::new(),
                        running: true,
                        started: Some(std::time::Instant::now()),
                        error: false,
                    },
                    String::new(),
                );
            }
            Event::Question {
                id,
                agent,
                questions,
            } => {
                self.question = Some(crate::app::question::PendingQuestion::new(
                    id, agent, questions,
                ));
                self.set_activity(Activity::Asking);
                self.auto_scroll = true;
            }
            Event::ToolResult {
                id,
                content,
                is_error,
                ..
            } => {
                // Answered here or not, the question is over once its call
                // has a result.
                if self.question.as_ref().is_some_and(|q| q.id == id) {
                    self.question = None;
                }
                if let Some(block) = self.conversation_mut().iter_mut().rev().find(|block| {
                    matches!(&block.kind, TranscriptKind::Tool { id: block_id, .. } if block_id == &id)
                }) {
                    if let TranscriptKind::Tool {
                        result,
                        running,
                        error,
                        ..
                    } = &mut block.kind
                    {
                        *result = content;
                        *running = false;
                        *error = is_error;
                    }
                }
                self.set_activity(Activity::Waiting);
            }
            Event::ToolProgress { id, delta } => {
                // Append the delta to the matching tool block's `result` so
                // the classify+render path picks up the growing preview on
                // the next frame. No new block gets created; if the tool is
                // already gone (rare race), the delta is silently dropped.
                if let Some(block) = self.conversation_mut().iter_mut().rev().find(|block| {
                    matches!(
                        &block.kind,
                        TranscriptKind::Tool { id: bid, .. } if bid == &id
                    )
                }) {
                    if let TranscriptKind::Tool { result, .. } = &mut block.kind {
                        result.push_str(&delta);
                    }
                }
            }
            Event::FormatterMissing {
                language,
                bin,
                install_hint,
                install_cmd,
            } => {
                self.push(
                    TranscriptKind::Notice,
                    format!(
                        "Formatter `{bin}` for {language} not installed. {install_hint}: `{}`",
                        install_cmd.join(" ")
                    ),
                );
            }
            Event::Notice { message } => self.push(TranscriptKind::Notice, message),
            Event::AgentSwitched { to, reason } => {
                // Pinned to where the transcript has reached rather than to a
                // turn index: the block list is what the marker is drawn
                // against, and a live switch lands between two blocks.
                self.logs.push(
                    crate::logs::LogKind::Agent,
                    format!("{} → {to}", self.agent_name),
                );
                let at = self.conversation_mut().len();
                self.switch_markers.push((
                    at,
                    enowx_core::session::AgentSwitch {
                        from: self.agent_name.clone(),
                        to: to.clone(),
                        reason,
                        at_turn: at,
                    },
                ));
                self.agent_name = to;
            }
            Event::DelegationStarted {
                agent,
                task,
                session_id,
            } => {
                self.push(
                    TranscriptKind::Brief {
                        agent: agent.clone(),
                        id: session_id.clone(),
                        state: crate::app::DelegationState::Running,
                        report: String::new(),
                    },
                    task.clone(),
                );
                self.logs.push(
                    crate::logs::LogKind::Agent,
                    format!("delegate → {agent}: {task}"),
                );
                self.delegations.push(crate::app::Delegation {
                    agent,
                    task,
                    session_id,
                    state: crate::app::DelegationState::Running,
                });
                self.select_tab(crate::ui::AGENTS_TAB);
            }
            Event::DelegationFinished {
                agent,
                summary,
                session_id,
                failed,
            } => {
                let outcome = if failed {
                    crate::app::DelegationState::Failed
                } else {
                    crate::app::DelegationState::Finished
                };
                // The report lands on the delegation's own row rather than as
                // a notice under it: one delegation, one block, from the
                // moment it starts to the moment it reports.
                let landed = self
                    .conversation_mut()
                    .iter_mut()
                    .rev()
                    .any(|block| match &mut block.kind {
                        TranscriptKind::Brief {
                            id, state, report, ..
                        } if *id == session_id => {
                            *state = outcome;
                            *report = summary.clone();
                            true
                        }
                        _ => false,
                    });
                if !landed {
                    // Finished without having started: it failed before the
                    // branch existed. The report still has to be seen.
                    self.push(
                        TranscriptKind::Brief {
                            agent: agent.clone(),
                            id: session_id.clone(),
                            state: outcome,
                            report: summary.clone(),
                        },
                        String::new(),
                    );
                }
                self.logs.push(
                    crate::logs::LogKind::Agent,
                    format!("{agent} {}", if failed { "failed" } else { "finished" }),
                );
                // Match on the branch id: the same agent can be delegated to
                // more than once in a session, and marking the first one
                // finished would leave a later run showing as done.
                if let Some(entry) = self
                    .delegations
                    .iter_mut()
                    .find(|d| d.session_id == session_id)
                {
                    entry.state = outcome;
                }
            }
            Event::Retry {
                message,
                attempt,
                max,
            } => {
                self.logs.push_with(
                    crate::logs::LogKind::Problem,
                    format!("retry {attempt}/{max}"),
                    Some(message.clone()),
                );
                self.retry_attempt = attempt;
                self.retry_max = max;
                self.push(TranscriptKind::Retry, message);
            }
            Event::Usage {
                input_tokens,
                output_tokens,
                context_tokens,
                context_window,
            } => {
                // One line per model call, with what it cost and how long it
                // took — the timing is why a turn felt slow, which nothing
                // else records.
                self.logs.push_with(
                    crate::logs::LogKind::Model,
                    format!(
                        "{} · {}",
                        self.model_label(),
                        crate::app::fmt_elapsed(self.turn_started.elapsed().as_secs())
                    ),
                    Some(format!(
                        "in {} · out {} · context {}",
                        crate::text::thousands(input_tokens as u64),
                        crate::text::thousands(output_tokens as u64),
                        crate::text::thousands(context_tokens as u64)
                    )),
                );
                self.tokens_in = input_tokens;
                self.tokens_out = output_tokens;
                self.context_tokens = context_tokens;
                if context_window > 0 {
                    self.context_window = context_window;
                }
            }
            Event::Trimmed { tool, was, now } => {
                self.logs.push_with(
                    crate::logs::LogKind::Context,
                    format!("trimmed {tool}"),
                    Some(format!(
                        "{} → {} chars",
                        crate::text::thousands(was as u64),
                        crate::text::thousands(now as u64)
                    )),
                );
                self.trimmed_count += 1;
                self.trimmed_saved += was.saturating_sub(now);
                // Not pushed to the transcript: this happens often enough
                // that a line each time would bury the conversation. The
                // sidebar carries the running total instead.
                self.status = format!(
                    "trimmed {tool} result: {} → {} chars",
                    crate::text::thousands(was as u64),
                    crate::text::thousands(now as u64),
                );
            }
            Event::Error { message } => {
                self.logs
                    .push(crate::logs::LogKind::Problem, message.clone());
                self.push(TranscriptKind::Error, message);
                self.question = None;
                self.busy = false;
                self.cancel = None;
                self.set_activity(Activity::Idle);
                self.status = "failed".into();
            }
            Event::Done { stop_reason } => {
                // A turn the user already stopped reports back when it has
                // finished unwinding. The UI moved on at the keypress, so
                // this must not relabel the status or disturb a turn the
                // user may have started since.
                if self.abandoned > 0 {
                    self.abandoned -= 1;
                    return;
                }
                self.question = None;
                self.busy = false;
                self.cancel = None;
                self.set_activity(Activity::Idle);
                self.status = stop_reason;
            }
        }
    }

    pub(crate) fn drain_events(&mut self) {
        loop {
            let next = self.events.as_mut().map(mpsc::Receiver::try_recv);
            match next {
                Some(Ok(event)) => self.apply_event(event),
                Some(Err(mpsc::error::TryRecvError::Disconnected)) => {
                    if self.busy {
                        self.push(
                            TranscriptKind::Error,
                            "Agent stopped without a terminal event",
                        );
                    }
                    self.set_activity(Activity::Idle);
                    self.busy = false;
                    self.events = None;
                    break;
                }
                _ => break,
            }
        }
    }
}

impl App {
    /// Close the thinking that was streaming, now that something else has
    /// begun, and note how long it took.
    fn finish_thinking(&mut self) {
        if let Some(block) = self.conversation_mut().last_mut() {
            if let TranscriptKind::Reasoning {
                started: Some(started),
                elapsed,
                ..
            } = &mut block.kind
            {
                if elapsed.is_none() {
                    *elapsed = Some(started.elapsed());
                }
            }
        }
    }
}
