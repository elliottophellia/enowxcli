use super::*;

impl App {
    /// Commands matching the modal's search box.
    ///
    /// Matches the summary as well as the name, so someone who remembers what
    /// a command does but not what it is called still finds it — which is the
    /// reason to open a palette rather than type the name.
    /// The palette's rows: every command, group by group, or while
    /// searching the matches, best first. A label that starts with what was
    /// typed beats one with a word that does, which beats one that merely
    /// contains it; a match in the summary comes last.
    pub(crate) fn palette_rows(&self) -> Vec<crate::commands::PaletteRow> {
        let rows = crate::commands::palette();
        let needle = self.modal_search.trim().to_lowercase();
        if needle.is_empty() {
            return rows;
        }
        let mut ranked: Vec<(u8, crate::commands::PaletteRow)> = rows
            .into_iter()
            .filter_map(|row| {
                let label = row.label.to_lowercase();
                let rank = if label.starts_with(&needle) || row.name.starts_with(&needle) {
                    0
                } else if label
                    .split_whitespace()
                    .any(|word| word.starts_with(&needle))
                {
                    1
                } else if label.contains(&needle) || row.name.contains(&needle) {
                    2
                } else if row.summary.to_lowercase().contains(&needle) {
                    3
                } else {
                    return None;
                };
                Some((rank, row))
            })
            .collect();
        ranked.sort_by_key(|(rank, _)| *rank);
        ranked.into_iter().map(|(_, row)| row).collect()
    }

    /// Run whichever command the palette has highlighted.
    pub(crate) fn accept_palette_row(&mut self) -> Result<()> {
        let rows = self.palette_rows();
        let Some(name) = rows.get(self.modal_cursor).map(|row| row.name) else {
            return Ok(());
        };
        self.modal = Modal::None;
        self.modal_search.clear();
        self.modal_cursor = 0;
        // Picking a command runs it, and a command whose job is to choose
        // something opens its own window to choose in. Staging `/name ` in
        // the composer instead would make the palette a way of typing, which
        // is what `/` already is.
        self.run_command(&format!("/{name}"))
    }

    /// The three things that can be done with a message already sent.
    pub(crate) const MESSAGE_ACTIONS: [(&'static str, &'static str); 3] = [
        ("Edit prompt", "Change it and send again from here"),
        ("Resend", "Send it again unchanged from here"),
        ("Copy", "Copy the text to the clipboard"),
    ];

    /// Open the action menu for the user message rendered as block `index`.
    pub(crate) fn open_message_menu(&mut self, index: usize) {
        if !self
            .blocks
            .get(index)
            .is_some_and(|b| matches!(b.kind, TranscriptKind::User))
        {
            return;
        }
        self.message_target = Some(index);
        self.modal_items = Self::MESSAGE_ACTIONS
            .iter()
            .map(|(label, hint)| ((*label).to_owned(), (*hint).to_owned()))
            .collect();
        self.modal_cursor = 0;
        self.modal = Modal::Message;
    }

    fn accept_message_action(&mut self) -> Result<()> {
        let Some(index) = self.message_target else {
            self.modal = Modal::None;
            return Ok(());
        };
        let text = self
            .blocks
            .get(index)
            .map(|b| b.text.clone())
            .unwrap_or_default();
        match self.modal_cursor {
            0 => {
                self.message_draft = text;
                self.message_draft_cursor = self.message_draft.len();
                self.modal = Modal::MessageEdit;
            }
            1 => {
                self.modal = Modal::None;
                return self.rewind_and_send(index, text);
            }
            2 => {
                self.modal = Modal::None;
                self.message_target = None;
                if crate::attachments::copy_to_clipboard(&text) {
                    self.status = format!("copied {} chars", text.len());
                } else {
                    self.status = "could not reach the clipboard".into();
                }
            }
            _ => self.modal = Modal::None,
        }
        Ok(())
    }

    fn submit_message_edit(&mut self) -> Result<()> {
        let Some(index) = self.message_target else {
            self.modal = Modal::None;
            return Ok(());
        };
        let text = self.message_draft.trim().to_owned();
        self.modal = Modal::None;
        if text.is_empty() {
            self.message_target = None;
            self.status = "empty prompt; nothing sent".into();
            return Ok(());
        }
        self.rewind_and_send(index, text)
    }

    /// Cut the conversation back to the message at `index` and send `text` in
    /// its place. Everything after it answered a question that is being
    /// replaced, so it goes too — on screen and in what the model is sent.
    fn rewind_and_send(&mut self, index: usize, text: String) -> Result<()> {
        if !self.rewind_to(index)? {
            return Ok(());
        }
        self.start_turn(text);
        Ok(())
    }

    /// Cut back to the message at `index`, on screen and in the stored
    /// session. Returns whether the cut happened; a running turn refuses,
    /// because its reply would attach to a message that is being removed.
    ///
    /// Separate from sending so the destructive half can be tested without a
    /// tokio runtime — `start_turn` spawns.
    pub(crate) fn rewind_to(&mut self, index: usize) -> Result<bool> {
        self.message_target = None;
        if self.busy {
            self.status = "Stop the current turn first".into();
            return Ok(false);
        }
        // Which user message this is, counted among user messages only: the
        // stored session has one turn per message, while the transcript has
        // extra blocks for tools and notices.
        let nth = self
            .blocks
            .iter()
            .take(index)
            .filter(|b| matches!(b.kind, TranscriptKind::User))
            .count();
        if let Some(id) = self.session_id.clone() {
            let mut session = self.store.load(&id)?;
            if let Some(turn) = session.user_turn_index(nth) {
                let removed = session.truncate_from(turn);
                self.store.save(&session)?;
                self.status = format!("rewound {} message(s)", removed.len());
            }
        }
        self.blocks.truncate(index);
        self.render_cache.truncate(index);
        self.auto_scroll = true;
        Ok(true)
    }

    /// TypeSafe's settings: the key, and the two features it powers.
    ///
    /// Not part of `/provider`. Jev answers typed questions, not prompts —
    /// listing it beside the chat providers would offer a model that cannot
    /// hold a conversation.
    pub(crate) fn open_typesafe(&mut self) {
        self.modal_items = vec![
            (
                "API key".into(),
                if !self.config.typesafe.active() {
                    "not set  ·  every feature below is off".into()
                } else if self.typesafe_check.is_some() {
                    "set  ·  checking…".into()
                } else if self.status.starts_with("TypeSafe key not working") {
                    // Say what is wrong where the user is looking, not only
                    // in the status line they may have scrolled past.
                    format!("set  ·  {}", &self.status["TypeSafe key not ".len()..])
                } else {
                    "set  ·  Enter to replace".into()
                },
            ),
            (
                "Trim spent tool results".into(),
                format!(
                    "{}  ·  keeps context small; the transcript is untouched",
                    on_off(self.config.typesafe.gate_tool_results)
                ),
            ),
            (
                "Keep live turns through compaction".into(),
                format!(
                    "{}  ·  folds what is finished rather than what is old",
                    on_off(self.config.typesafe.rank_compaction)
                ),
            ),
        ];
        // Say plainly whether anything has actually happened this session.
        // A feature that removes text from the model's context and shows
        // nothing for it cannot be told apart from one that is broken.
        self.modal_items.push((
            "This session".into(),
            // What happened comes first: the count is a fact about this
            // session, not a function of the key's current state.
            if self.trimmed_count == 0 && !self.config.typesafe.active() {
                "nothing trimmed — no key set".into()
            } else if self.trimmed_count == 0 {
                "nothing trimmed yet".into()
            } else {
                format!(
                    "{} result(s) trimmed · {} characters saved",
                    self.trimmed_count,
                    crate::text::thousands(self.trimmed_saved as u64)
                )
            },
        ));
        self.modal_cursor = 0;
        self.modal = Modal::TypeSafe;
        if !self.config.typesafe.active() {
            self.status = "TypeSafe is off until a key is set".into();
        }
    }

    fn accept_typesafe_row(&mut self) -> Result<()> {
        match self.modal_cursor {
            0 => {
                // Reuse the key form, which already masks input: its one
                // field is the key.
                self.settings.api_key.clear();
                self.open_form(Modal::TypeSafeKey);
            }
            1 => {
                let mut next = self.config.clone();
                next.typesafe.gate_tool_results = !next.typesafe.gate_tool_results;
                next.save()?;
                self.adopt(next);
                self.open_typesafe();
            }
            2 => {
                let mut next = self.config.clone();
                next.typesafe.rank_compaction = !next.typesafe.rank_compaction;
                next.save()?;
                self.adopt(next);
                self.open_typesafe();
            }
            // Row 3 is the session report, not an action.
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn save_typesafe_key(&mut self) -> Result<()> {
        let key = self.settings.api_key.trim().to_owned();
        let mut next = self.config.clone();
        next.typesafe.api_key = key.clone();
        next.save()?;
        self.adopt(next);
        self.settings.api_key.clear();
        if key.is_empty() {
            self.status = "TypeSafe key cleared; its features are off".into();
            self.typesafe_check = None;
            self.open_typesafe();
            return Ok(());
        }
        // A saved key that does not work is worse than no key: every feature
        // it powers fails silently back to the old behaviour, so nothing
        // looks wrong. Check it now and say so.
        let config = self.config.typesafe.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        // Saving the key must not depend on being able to check it. Outside a
        // tokio runtime there is nothing to spawn onto, and a key that saved
        // everywhere except there would be a worse bug than an unchecked one.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                self.status = "checking the TypeSafe key…".into();
                self.typesafe_check = Some(rx);
                handle.spawn(async move {
                    let result = match enowx_core::systemone::SystemOne::new(&config) {
                        Some(client) => client.check().await,
                        None => Err("no key".into()),
                    };
                    let _ = tx.send(result).await;
                });
            }
            Err(_) => {
                self.typesafe_check = None;
                self.status = "TypeSafe key saved".into();
            }
        }
        self.open_typesafe();
        Ok(())
    }

    /// Fold in the key check once it lands. Called from the frame loop.
    pub(crate) fn drain_typesafe_check(&mut self) {
        let Some(rx) = self.typesafe_check.as_mut() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(())) => {
                self.status = "TypeSafe key works".into();
                self.typesafe_check = None;
                if self.modal == Modal::TypeSafe {
                    self.open_typesafe();
                }
            }
            Ok(Err(reason)) => {
                self.status = format!("TypeSafe key not working: {reason}");
                self.typesafe_check = None;
                if self.modal == Modal::TypeSafe {
                    self.open_typesafe();
                }
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                self.typesafe_check = None;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }
    }

    pub(crate) fn open_palette(&mut self) {
        self.modal = Modal::Commands;
        self.palette_offset = 0;
        self.modal_cursor = 0;
        self.modal_search.clear();
        self.modal_error.clear();
    }

    pub(crate) fn command_matches(&self) -> Vec<(&'static str, &'static str)> {
        // While a question waits, what is typed is the answer, commands
        // included.
        if self.question.is_some()
            || !self.input.starts_with('/')
            || self.input.contains(char::is_whitespace)
        {
            return Vec::new();
        }
        COMMANDS
            .iter()
            .copied()
            .filter(|(name, _)| name.starts_with(&self.input[1..]))
            .collect()
    }

    pub(crate) fn run_command(&mut self, line: &str) -> Result<()> {
        let command = line.trim_start_matches('/');
        let (name, args) = command.split_once(' ').unwrap_or((command, ""));
        // A running turn belongs to the agent that started it; swapping
        // underneath it would attribute its results to the wrong one.
        if self.busy && matches!(name, "new" | "resume" | "provider" | "model" | "agent") {
            self.status = "Stop the current turn before changing session or configuration".into();
            return Ok(());
        }
        match name {
            "help" => {
                let mut text = String::from("Commands");
                for (name, summary) in COMMANDS { text.push_str(&format!("\n  /{name:<10} {summary}")); }
                // Every key has a form that reaches us on Linux, macOS and
                // Windows terminals alike; the second form is the one to use
                // where the first is taken by the OS or the terminal.
                text.push_str(concat!(
                    "\n\nKeys",
                    "\n  Enter              Send (queues while a turn runs)",
                    "\n  Shift/Alt+Enter    Newline (Ctrl+J too, when idle)",
                    "\n  Ctrl+Enter, Ctrl+S Send now, while a turn runs",
                    "\n  Ctrl+Backspace     Erase a word (Alt+Backspace too)",
                    "\n  Ctrl+R / Ctrl+O    Toggle reasoning / tool output",
                    "\n  PgUp/PgDn          Scroll transcript",
                    "\n  Esc                Close page / stop turn / clear input",
                    "\n  Ctrl+C             Stop turn / clear input / quit",
                    "\n  Ctrl+D             Quit when the composer is empty",
                    "\n  Ctrl+P             Chat / Settings (Left: section list)",
                    "\n  Ctrl+T             Next sidebar tab (Alt+1-4 picks one)",
                    "\n  Ctrl+G / Ctrl+X    Filter logs / show detail",
                    "\n  Alt+Left/Right     Sidebar pages (Alt+B / Alt+F too)",
                    "\n  Ctrl+B             Toggle sidebar",
                    "\n  Ctrl+Up, Alt+Up    Edit, resend or copy your last message",
                    "\n  Ctrl+V, Alt+V      Attach an image from the clipboard",
                    "\n  /commands          Search every command",
                ));
                self.push(TranscriptKind::System, text);
            }
            "new" => self.new_session(),
            "resume" => self.open_sessions()?,
            "agent" if !args.trim().is_empty() => self.force_agent(args.trim())?,
            "agent" => self.open_agents(),
            "model" if !args.trim().is_empty() => self.choose_model(args.trim())?,
            "model" => self.open_model_picker(None),
            "provider" => self.open_providers(),
            "attach" if !args.trim().is_empty() => {
                self.attach_from_path(std::path::Path::new(args.trim()));
                if let Some(error) = self.attach_error.clone() {
                    anyhow::bail!(error);
                }
            }
            "attach" => self.open_attach()?,
            "typesafe" => self.open_typesafe(),
            "theme" => self.open_themes(),
            "effort" if !args.trim().is_empty() => self.choose_effort(args)?,
            "effort" => self.open_effort()?,
            "skills" => self.open_skills(),
            "mcp" => self.open_mcp(),
            "rag" => self.open_page(crate::app::pages::Page::Rag)?,
            "team" => self.open_page(crate::app::pages::Page::Team)?,
            "update" => self.update_now(),
            "compact" => self.start_compact()?,
            "handoff" => self.open_handoff()?,
            "commands" => self.open_palette(),
            "sidebar" => self.toggle_sidebar()?,
            "preview" => self.toggle_preview()?,
            "reasoning" => {
                self.show_reasoning = !self.show_reasoning;
                self.status = format!(
                    "thinking {}",
                    if self.show_reasoning { "open" } else { "closed" }
                );
            }
            "tools" => {
                self.show_tool_output = !self.show_tool_output;
                self.status = format!("tool output {}", if self.show_tool_output { "expanded" } else { "compact" });
            }
            "clear" => { self.blocks.clear(); self.status = "transcript cleared".into(); }
            "stop" => self.interrupt(),
            "retry" => self.continue_turn(),
            "status" => self.push(TranscriptKind::System, format!(
                "Model: {} · {}\nAgent: {}\nWorkspace: {}\nSession: {}\nTokens: {} in / {} out\nTheme: {}\nThinking: {}",
                self.config.model.active,
                self.config.active_connection().map(|c| c.name).unwrap_or_else(|| "no provider".into()),
                self.active_agent(), self.config.workspace().display(),
                self.session_id.as_deref().unwrap_or("(new)"), self.tokens_in, self.tokens_out, self.theme.name,
                if self.show_reasoning { "open" } else { "closed" },
            )),
            "quit" | "exit" => self.should_quit = true,
            "" => {}
            other => self.push(TranscriptKind::Error, format!("Unknown command /{other}. Try /help.")),
        }
        Ok(())
    }
    /// The thinking efforts the model in use offers, as models.dev lists
    /// them, with the provider's default first.
    pub(crate) fn open_effort(&mut self) -> Result<()> {
        let model = &self.config.model;
        // Said on the status line rather than as an error: a model without
        // levels is a fact about it, not something that went wrong.
        if model.efforts.is_empty() {
            self.status = if model.active.is_empty() {
                "No model is in use: pick one with /model first".to_owned()
            } else {
                format!(
                    "{} has no thinking effort to choose (models.dev lists none)",
                    model.active
                )
            };
            return Ok(());
        }
        self.modal_items = std::iter::once((
            "default".to_owned(),
            "Whatever the provider does when none is asked for".to_owned(),
        ))
        .chain(model.efforts.iter().map(|level| {
            let what = match level.as_str() {
                "none" => "No thinking: fastest, for simple turns",
                "minimal" => "Barely any thinking",
                "low" => "Light thinking: quick and cheap",
                "medium" => "Balanced",
                "high" => "Thorough: slower, more tokens",
                "xhigh" => "Very thorough",
                "max" => "As much as the model allows: slowest and costliest",
                _ => "",
            };
            (level.clone(), what.to_owned())
        }))
        .collect();
        self.modal_cursor = self
            .modal_items
            .iter()
            .position(|(level, _)| {
                *level == model.effort || (model.effort.is_empty() && level == "default")
            })
            .unwrap_or(0);
        self.modal = Modal::Effort;
        Ok(())
    }

    /// Ask the model in use to think at `level` from the next call on.
    pub(crate) fn choose_effort(&mut self, level: &str) -> Result<()> {
        let mut next = self.config.clone();
        next.set_effort(level)?;
        let shown = if next.model.effort.is_empty() {
            "provider default".to_owned()
        } else {
            next.model.effort.clone()
        };
        self.adopt(next);
        self.modal = Modal::None;
        self.status = format!("thinking effort: {shown}");
        Ok(())
    }

    pub(crate) fn open_themes(&mut self) {
        self.modal = Modal::Themes;
        self.modal_cursor = THEMES
            .iter()
            .position(|t| t.name == self.theme.name)
            .unwrap_or(0);
        self.modal_items = THEMES
            .iter()
            .map(|t| (t.label.into(), format!("Palette id: {}", t.name)))
            .collect();
    }

    /// `m` in the roster: choose the selected agent's own model from the
    /// model list. The conversation's model is left as it is.
    pub(crate) fn pick_agent_model(&mut self) {
        let Some((agent, _)) = self.modal_items.get(self.modal_cursor).cloned() else {
            return;
        };
        let current = self.config.agent.models.get(&agent).cloned();
        self.open_model_picker(None);
        if self.modal != Modal::Models {
            return;
        }
        self.picking_for_agent = Some(agent);
        if let Some(current) = current {
            let rows = self.picker_rows();
            if let Some(index) = rows.iter().position(|row| {
                matches!(row, super::model_picker::PickerRow::Model { model, .. } if *model == current)
            }) {
                self.modal_cursor = index;
            }
        }
    }

    /// Give `agent` the model `model`, or none of its own (`None`): it runs on
    /// its tier's model, or the conversation's.
    pub(crate) fn set_agent_model(&mut self, agent: &str, model: Option<&str>) -> Result<()> {
        let mut next = self.config.clone();
        match model {
            Some(model) => {
                next.agent.models.insert(agent.to_owned(), model.to_owned());
            }
            None => {
                next.agent.models.remove(agent);
            }
        }
        next.save()?;
        self.adopt(next);
        let name = enowx_core::agent_def::display_name(agent);
        self.status = match model {
            Some(model) => format!("{name} runs on {model}"),
            None => format!("{name} runs on the default model again"),
        };
        Ok(())
    }

    /// `d` in the roster: the selected agent drops its own model.
    pub(crate) fn clear_agent_model(&mut self) -> Result<()> {
        let Some((agent, _)) = self.modal_items.get(self.modal_cursor).cloned() else {
            return Ok(());
        };
        if !self.config.agent.models.contains_key(&agent) {
            self.status = format!(
                "{} has no model of its own",
                enowx_core::agent_def::display_name(&agent)
            );
            return Ok(());
        }
        self.set_agent_model(&agent, None)
    }

    /// Back to the roster, on `agent`, after choosing its model.
    pub(crate) fn return_to_agents(&mut self, agent: &str) {
        self.open_agents();
        if let Some(index) = self.modal_items.iter().position(|(name, _)| name == agent) {
            self.modal_cursor = index;
        }
    }

    /// The roster as a picker. `compactor` is left out: it is machinery the
    /// session runs on its own, not something to hand a request to.
    pub(crate) fn open_agents(&mut self) {
        let active = self.active_agent().to_owned();
        self.modal_items = self
            .discovery
            .agents
            .iter()
            .filter(|a| a.is_routable())
            .map(|a| (a.name.clone(), a.description.clone()))
            .collect();
        self.modal_cursor = self
            .modal_items
            .iter()
            .position(|(name, _)| *name == active)
            .unwrap_or(0);
        self.modal = Modal::Agents;
    }

    pub(crate) fn accept_modal(&mut self) -> Result<()> {
        match self.modal {
            Modal::Providers => self.select_provider(),
            Modal::ProviderKey => return self.connect_key(),
            Modal::ProviderForm => return self.save_provider_form(),
            Modal::ModelManual => return self.save_manual_model(),
            Modal::ModelEdit => return self.save_edit_model(),
            Modal::Sessions => {
                if let Some((id, _)) = self.modal_items.get(self.modal_cursor).cloned() {
                    self.resume(&id)?;
                }
                self.modal = Modal::None;
            }
            Modal::Models => {
                if let Some(model) = self.selected_model() {
                    return self.choose_model(&model);
                }
            }
            Modal::Themes => {
                self.select_theme(self.modal_cursor)?;
                self.modal = Modal::None;
            }
            Modal::Effort => {
                if let Some((level, _)) = self.modal_items.get(self.modal_cursor).cloned() {
                    return self.choose_effort(&level);
                }
                self.modal = Modal::None;
            }
            Modal::Attach => {
                if let Some((path, _)) = self.modal_items.get(self.modal_cursor).cloned() {
                    self.attach_from_path(std::path::Path::new(&path));
                }
                self.modal = Modal::None;
                if let Some(error) = self.attach_error.clone() {
                    anyhow::bail!(error);
                }
            }
            Modal::Skills => return self.read_selected_skill(),
            Modal::Agents => {
                if let Some((name, _)) = self.modal_items.get(self.modal_cursor).cloned() {
                    self.modal = Modal::None;
                    return self.force_agent(&name);
                }
                self.modal = Modal::None;
            }
            Modal::TypeSafe => return self.accept_typesafe_row(),
            Modal::TypeSafeKey => return self.save_typesafe_key(),
            Modal::Message => return self.accept_message_action(),
            Modal::MessageEdit => return self.submit_message_edit(),
            Modal::Commands => return self.accept_palette_row(),
            Modal::Mcp => return self.accept_mcp_row(),
            Modal::McpForm => return self.submit_mcp_form(),
            Modal::BuiltinMcp => return self.save_builtin_mcp(),
            Modal::Rag => return self.save_rag(),
            Modal::Team => return self.save_team(),
            Modal::Updates => return self.save_updates(),
            Modal::QuitConfirm => {
                self.should_quit = true;
            }
            Modal::Handoff => {
                let choice = self
                    .modal_items
                    .get(self.modal_cursor)
                    .map(|(id, _)| id.clone());
                self.modal = Modal::None;
                match choice.as_deref() {
                    Some("keep") => return self.start_handoff(false),
                    Some("delete") => return self.start_handoff(true),
                    _ => {}
                }
            }
            Modal::None => {}
        }
        Ok(())
    }

    /// Fire off a manual compact for the current session. Runs off the UI
    /// thread so a slow summarizer never freezes the terminal; the result
    /// arrives via a Notice event.
    pub(crate) fn start_compact(&mut self) -> anyhow::Result<()> {
        let Some(id) = self.session_id.clone() else {
            self.push(
                crate::session::TranscriptKind::Notice,
                "no active session yet — send one message first",
            );
            return Ok(());
        };
        if self.busy {
            anyhow::bail!("a turn is already running; wait or interrupt first");
        }
        let agent = self.agent.clone();
        let (tx, rx) = tokio::sync::mpsc::channel::<enowx_core::Event>(64);
        self.events = Some(rx);
        self.status = "compacting…".into();
        self.busy = true;
        tokio::spawn(async move {
            let notice = match agent.compact(&id).await {
                Ok(Some(_)) => "compact done: older turns folded".to_string(),
                Ok(None) => "compact skipped: not enough history".to_string(),
                Err(e) => format!("compact failed: {e:#}"),
            };
            let _ = tx.send(enowx_core::Event::Notice { message: notice }).await;
            let _ = tx
                .send(enowx_core::Event::Done {
                    stop_reason: "compact".into(),
                })
                .await;
        });
        Ok(())
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}
