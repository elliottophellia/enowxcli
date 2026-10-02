use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    pub(crate) fn key(&mut self, key: KeyEvent) -> Result<()> {
        // Stopping the model outranks whatever window happens to be in front
        // of it: a modal handler that ate this key left Ctrl+C doing nothing
        // while a turn ran.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c'))
            && self.busy
        {
            // Looking at a sub-agent's transcript, Ctrl+C means "get me out of
            // here" before it means "stop the turn". Stopping the conversation
            // from inside a window that is not the conversation is not what
            // the key is being pressed for, and the sub-agent on screen is not
            // something the user drives anyway.
            if self.viewing.is_some() {
                self.leave_delegation();
                return Ok(());
            }
            self.interrupt();
            return Ok(());
        }
        // Ctrl+P switches between Chat and Settings from anywhere, a section
        // included: a section is a modal, and its own handler would otherwise
        // take the key.
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('p')
            && self.modal != Modal::QuitConfirm
        {
            return self.switch_tab();
        }
        if self.settings_page_key(&key)? {
            return Ok(());
        }
        if self.modal != Modal::None {
            if self.modal.is_form() && self.modal != Modal::McpForm {
                return self.settings_key(key);
            }
            if self.modal == Modal::QuitConfirm {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.should_quit = true;
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.modal = Modal::None;
                    }
                    KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::BackTab => {
                        // Flip between Yes and No.
                        self.quit_confirm_yes = !self.quit_confirm_yes;
                    }
                    KeyCode::Enter => {
                        if self.quit_confirm_yes {
                            self.should_quit = true;
                        } else {
                            self.modal = Modal::None;
                        }
                    }
                    _ => {}
                }
                return Ok(());
            }
            if self.modal == Modal::Themes {
                match key.code {
                    KeyCode::Esc => {
                        self.theme = Theme::find(&self.config.ui.theme);
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.theme = Theme::find(&self.config.ui.theme);
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Up => {
                        self.modal_cursor = self.modal_cursor.saturating_sub(1);
                        if let Some(t) = THEMES.get(self.modal_cursor) {
                            self.theme = *t;
                        }
                        return Ok(());
                    }
                    KeyCode::Down if self.modal_cursor + 1 < self.modal_items.len() => {
                        self.modal_cursor += 1;
                        if let Some(t) = THEMES.get(self.modal_cursor) {
                            self.theme = *t;
                        }
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        return self.accept_modal();
                    }
                    _ => return Ok(()),
                }
            }
            if self.modal == Modal::Models {
                let control = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Esc => {
                        if self.modal_search.is_empty() {
                            self.modal = Modal::None;
                            self.picker.events = None;
                            // Choosing for an agent: back to the roster.
                            if let Some(agent) = self.picking_for_agent.take() {
                                self.return_to_agents(&agent);
                            }
                        } else {
                            self.search_models(String::clear);
                        }
                    }
                    KeyCode::Char('c') if control => {
                        self.modal = Modal::None;
                        self.picker.events = None;
                        self.picking_for_agent = None;
                    }
                    KeyCode::Char('f') if control => self.toggle_favorite_model()?,
                    KeyCode::Up => self.move_picker(-1),
                    KeyCode::Down => self.move_picker(1),
                    KeyCode::PageUp => self.move_picker(-10),
                    KeyCode::PageDown => self.move_picker(10),
                    // Ctrl, not F-keys: not every terminal or OS passes
                    // function keys through.
                    KeyCode::Char('r') if control => self.refresh_models(),
                    KeyCode::Char('n') if control => self.open_manual_model(),
                    KeyCode::Char('e') if control => self.open_edit_model(),
                    KeyCode::Enter => {
                        if let Some(model) = self.selected_model() {
                            self.choose_model(&model)?;
                        }
                    }
                    KeyCode::Backspace => self.search_models(|search| {
                        search.pop();
                    }),
                    KeyCode::Char(c) if !control => self.search_models(|search| search.push(c)),
                    _ => {}
                }
                return Ok(());
            }
            if self.modal == Modal::Providers
                && matches!(key.code, KeyCode::Delete | KeyCode::Char('d'))
            {
                return self.disconnect_provider();
            }
            if self.modal == Modal::Commands {
                match key.code {
                    KeyCode::Esc => {
                        self.modal = Modal::None;
                        self.modal_search.clear();
                        return Ok(());
                    }
                    KeyCode::Up => {
                        self.modal_cursor = self.modal_cursor.saturating_sub(1);
                        return Ok(());
                    }
                    KeyCode::Down => {
                        if self.modal_cursor + 1 < self.palette_rows().len() {
                            self.modal_cursor += 1;
                        }
                        return Ok(());
                    }
                    KeyCode::Backspace => {
                        self.modal_search.pop();
                        // A narrower list can leave the cursor past its end.
                        self.modal_cursor = 0;
                        return Ok(());
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.modal_search.push(c);
                        self.modal_cursor = 0;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            if self.modal == Modal::Skills || self.modal == Modal::Mcp {
                match key.code {
                    KeyCode::Esc => {
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.modal = Modal::None;
                        return Ok(());
                    }
                    // Enter edits the entry, so the tool listing moves to its
                    // own key rather than disappearing.
                    KeyCode::Char('t') if self.modal == Modal::Mcp => {
                        self.show_mcp_tools();
                        return Ok(());
                    }
                    // Configure a built-in server's credentials.
                    KeyCode::Char('c') if self.modal == Modal::Mcp => {
                        self.config_selected_mcp();
                        return Ok(());
                    }
                    KeyCode::Up => {
                        self.modal_cursor = self.modal_cursor.saturating_sub(1);
                        return Ok(());
                    }
                    KeyCode::Down => {
                        let len = if self.modal == Modal::Skills {
                            self.skill_rows().len()
                        } else {
                            self.mcp_rows().len()
                        };
                        if self.modal_cursor + 1 < len {
                            self.modal_cursor += 1;
                        }
                        return Ok(());
                    }
                    KeyCode::Tab => {
                        if self.modal == Modal::Skills {
                            self.toggle_selected_skill()?;
                        } else {
                            self.toggle_selected_mcp()?;
                        }
                        return Ok(());
                    }
                    KeyCode::Enter => return self.accept_modal(),
                    KeyCode::Backspace => {
                        self.modal_search.pop();
                        self.modal_cursor = 0;
                        return Ok(());
                    }
                    KeyCode::Char(c)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        self.modal_search.push(c);
                        self.modal_cursor = 0;
                        return Ok(());
                    }
                    _ => return Ok(()),
                }
            }
            if self.modal == Modal::McpForm {
                return self.mcp_form_key(key);
            }
            if self.modal == Modal::MessageEdit {
                match key.code {
                    KeyCode::Esc => {
                        self.modal = Modal::None;
                        self.message_target = None;
                        self.message_draft.clear();
                    }
                    KeyCode::Enter => return self.accept_modal(),
                    KeyCode::Backspace => {
                        if self.message_draft_cursor > 0 {
                            // Step a whole character, not a byte: a draft can
                            // hold anything the composer can.
                            let prev = self.message_draft[..self.message_draft_cursor]
                                .chars()
                                .next_back()
                                .map(char::len_utf8)
                                .unwrap_or(0);
                            self.message_draft_cursor -= prev;
                            self.message_draft.remove(self.message_draft_cursor);
                        }
                    }
                    KeyCode::Left => {
                        if self.message_draft_cursor > 0 {
                            let prev = self.message_draft[..self.message_draft_cursor]
                                .chars()
                                .next_back()
                                .map(char::len_utf8)
                                .unwrap_or(0);
                            self.message_draft_cursor -= prev;
                        }
                    }
                    KeyCode::Right => {
                        if self.message_draft_cursor < self.message_draft.len() {
                            let next = self.message_draft[self.message_draft_cursor..]
                                .chars()
                                .next()
                                .map(char::len_utf8)
                                .unwrap_or(0);
                            self.message_draft_cursor += next;
                        }
                    }
                    KeyCode::Home => self.message_draft_cursor = 0,
                    KeyCode::End => self.message_draft_cursor = self.message_draft.len(),
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.message_draft.insert(self.message_draft_cursor, c);
                        self.message_draft_cursor += c.len_utf8();
                    }
                    _ => {}
                }
                return Ok(());
            }
            match key.code {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.modal = Modal::None
                }
                KeyCode::Up => self.modal_cursor = self.modal_cursor.saturating_sub(1),
                KeyCode::Down if self.modal_cursor + 1 < self.modal_items.len() => {
                    self.modal_cursor += 1
                }
                KeyCode::Enter => self.accept_modal()?,
                // The roster: the selected agent's own model, or none.
                KeyCode::Char('m') if self.modal == Modal::Agents => self.pick_agent_model(),
                KeyCode::Char('d') if self.modal == Modal::Agents => self.clear_agent_model()?,
                _ => {}
            }
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') if self.busy => self.interrupt(),
                KeyCode::Char('c') => {
                    if self.modal == Modal::QuitConfirm {
                        // Second Ctrl+C on the confirm popup: quit.
                        self.should_quit = true;
                    } else if !self.input.is_empty() {
                        // Composer has content: clear it instead of quitting.
                        self.input.clear();
                        self.cursor = 0;
                        self.status = "input cleared".into();
                    } else {
                        // Empty composer: open the confirm popup.
                        self.modal = Modal::QuitConfirm;
                    }
                }
                // No mouse needed: this is a TUI, and plenty of sessions run
                // over ssh or inside tmux without one. macOS takes Ctrl+Up
                // for Mission Control, so Alt+Up does the same.
                KeyCode::Up => self.open_last_message_menu(),
                // Ctrl+D is end-of-input: it quits from an empty composer and
                // erases forward otherwise, as in a shell, so a stray press
                // mid-draft does not throw the session away.
                KeyCode::Char('d') if self.input.is_empty() => self.should_quit = true,
                KeyCode::Char('d') => self.delete_forward(),
                KeyCode::Backspace => self.delete_word_back(),
                // Ctrl+S sends now on every terminal: Ctrl+Enter only reaches
                // us where the terminal can tell it from Enter.
                KeyCode::Char('s') if self.busy && self.viewing.is_none() => self.send_now(),
                KeyCode::Char('l') => self.blocks.clear(),
                KeyCode::Char('r') => self.show_reasoning = !self.show_reasoning,
                KeyCode::Char('o') => self.show_tool_output = !self.show_tool_output,
                // Ctrl+V is not the primary paste key on macOS or most GUI
                // terminals; keep it as a fallback for setups where the native
                // shortcut cannot reach us, and let the terminal's own paste
                // (Cmd+V / Shift+Insert) flow through as a bracketed paste.
                // Windows Terminal keeps Ctrl+V for its own paste; Alt+V
                // attaches from the clipboard there.
                KeyCode::Char('v') => self.attach_from_clipboard(),
                KeyCode::Char('b') => self.toggle_sidebar()?,
                // Chat and Settings, the tabs at the top right; typing `/` lists
                // the commands.
                KeyCode::Char('p') => self.switch_tab()?,
                // Sidebar tabs and the log's filter and detail. These were
                // F1-F7; not every terminal or OS passes function keys
                // through, so they are Ctrl combinations now.
                KeyCode::Char('t') => self.next_tab(),
                KeyCode::Char('g') => {
                    self.log_filter = (self.log_filter + 1) % crate::logs::FILTERS.len();
                    self.select_tab(crate::ui::LOG_TAB);
                }
                KeyCode::Char('x') => {
                    self.log_detail = !self.log_detail;
                    self.select_tab(crate::ui::LOG_TAB);
                }
                // While a turn runs, Ctrl+Enter sends now: what is typed, or
                // else the first queued message, stopping the turn. Idle, it
                // inserts a newline. Many terminals report it as Ctrl+J, so
                // both reach the same handler.
                // Nothing is sent from a sub-agent's read-only transcript.
                KeyCode::Enter | KeyCode::Char('j') if self.viewing.is_some() => {}
                KeyCode::Enter | KeyCode::Char('j') if self.busy => self.send_now(),
                KeyCode::Enter | KeyCode::Char('j') => {
                    self.input.insert(self.cursor, '\n');
                    self.cursor += 1;
                }
                _ => {}
            }
            return Ok(());
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                // A newline at any time, including while a turn runs, when
                // Ctrl+Enter sends instead.
                KeyCode::Enter => {
                    self.input.insert(self.cursor, '\n');
                    self.cursor += 1;
                    return Ok(());
                }
                KeyCode::Char(c @ '1'..='4') => {
                    self.select_tab(c as usize - '1' as usize);
                    return Ok(());
                }
                // macOS Terminal and iTerm2 send Option+Left/Right as the
                // word-motion keys Alt+B and Alt+F, so both forms page.
                KeyCode::Left | KeyCode::Char('b') => {
                    self.page_sidebar(false);
                    return Ok(());
                }
                KeyCode::Right | KeyCode::Char('f') => {
                    self.page_sidebar(true);
                    return Ok(());
                }
                KeyCode::Up if self.viewing.is_none() => {
                    self.open_last_message_menu();
                    return Ok(());
                }
                KeyCode::Char('v') if self.viewing.is_none() => {
                    self.attach_from_clipboard();
                    return Ok(());
                }
                KeyCode::Backspace if self.viewing.is_none() => {
                    self.delete_word_back();
                    return Ok(());
                }
                // Any other Alt chord is not text: typing its letter would
                // put a stray character in the draft.
                KeyCode::Char(_) => return Ok(()),
                _ => {}
            }
        }

        // A sub-agent's transcript is read only, with no composer: Esc goes
        // back, the page keys scroll, and nothing else is typed or sent.
        if self.viewing.is_some() {
            match key.code {
                KeyCode::Esc => self.leave_delegation(),
                KeyCode::PageUp | KeyCode::Up => {
                    self.auto_scroll = false;
                    let step = if key.code == KeyCode::Up { 1 } else { 10 };
                    self.scroll = self.scroll.saturating_sub(step);
                }
                KeyCode::PageDown | KeyCode::Down => {
                    let step = if key.code == KeyCode::Down { 1 } else { 10 };
                    self.scroll = self.scroll.saturating_add(step).min(self.max_scroll);
                    self.auto_scroll = self.scroll == self.max_scroll;
                }
                _ => {}
            }
            return Ok(());
        }

        // A question from the agent takes Enter, the arrows and the digits
        // while it waits; anything else types the user's own answer.
        if self.question.is_some() && self.question_key(key) {
            return Ok(());
        }

        let matches = self.command_matches();
        if !matches.is_empty() {
            match key.code {
                KeyCode::Up => {
                    self.palette_cursor = (self.palette_cursor + matches.len() - 1) % matches.len();
                    return Ok(());
                }
                KeyCode::Down => {
                    self.palette_cursor = (self.palette_cursor + 1) % matches.len();
                    return Ok(());
                }
                KeyCode::Tab => {
                    self.input = format!(
                        "/{} ",
                        matches[self.palette_cursor.min(matches.len() - 1)].0
                    );
                    self.cursor = self.input.len();
                    return Ok(());
                }
                KeyCode::Enter => {
                    let command =
                        format!("/{}", matches[self.palette_cursor.min(matches.len() - 1)].0);
                    self.input.clear();
                    self.cursor = 0;
                    self.palette_cursor = 0;
                    return self.run_command(&command);
                }
                _ => self.palette_cursor = 0,
            }
        }

        match key.code {
            // Shift+Enter is a newline wherever the terminal reports it.
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.insert(self.cursor, '\n');
                self.cursor += 1;
            }
            KeyCode::Enter => {
                let text = self.input.trim().to_string();
                if self.busy && !text.starts_with('/') {
                    // Typed while the turn runs: it waits its turn.
                    if !text.is_empty() {
                        self.enqueue(text);
                    }
                } else if !text.is_empty() {
                    self.input.clear();
                    self.cursor = 0;
                    if text.starts_with('/') {
                        self.run_command(&text)?;
                    } else {
                        self.start_turn(text);
                    }
                }
            }
            KeyCode::Char(character) => {
                self.input.insert(self.cursor, character);
                self.cursor += character.len_utf8();
            }
            KeyCode::Backspace if self.cursor > 0 => {
                // Treat a chip like `[Image 2]` as a single glyph so users can
                // remove an attachment with one Backspace, no separate command.
                if let Some((range, index)) = crate::attachments::chip_at(&self.input, self.cursor)
                {
                    self.input.replace_range(range.clone(), "");
                    self.cursor = range.start;
                    if index < self.attachments.len() {
                        self.attachments.remove(index);
                    }
                    self.renumber_chips();
                } else {
                    let previous = self.input[..self.cursor]
                        .char_indices()
                        .last()
                        .map(|(index, _)| index)
                        .unwrap_or(0);
                    self.input.drain(previous..self.cursor);
                    self.cursor = previous;
                }
            }
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Left if self.cursor > 0 => {
                self.cursor = self.input[..self.cursor]
                    .char_indices()
                    .last()
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            }
            KeyCode::Right if self.cursor < self.input.len() => {
                self.cursor = self.input[self.cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(index, _)| self.cursor + index)
                    .unwrap_or(self.input.len())
            }
            KeyCode::Home => self.cursor = line_start(&self.input, self.cursor),
            KeyCode::End => self.cursor = line_end(&self.input, self.cursor),
            KeyCode::Up => {
                if !self.unqueue_last() {
                    self.cursor = move_line(&self.input, self.cursor, -1);
                }
            }
            KeyCode::Down => {
                self.cursor = move_line(&self.input, self.cursor, 1);
            }
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(10).min(self.max_scroll);
                self.auto_scroll = self.scroll == self.max_scroll;
            }
            KeyCode::Esc if self.viewing.is_some() => {
                // Looking at a sub-agent's transcript: the way back comes
                // before both other meanings of the key. Neither stopping a
                // turn nor clearing the composer is what someone reaching for
                // Esc here wants.
                self.leave_delegation();
            }
            KeyCode::Esc => {
                // While a turn runs, Esc stops it. Clearing the composer is
                // the idle meaning of the key, and discarding a draft is the
                // wrong thing to do to someone reaching for the stop key.
                if self.busy {
                    self.interrupt();
                } else if self.auto_retry.take().is_some() {
                    self.status = "automatic retry cancelled; /retry continues".into();
                } else {
                    self.input.clear();
                    self.cursor = 0;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Byte offset of the start of the line containing `cursor`.
fn line_start(text: &str, cursor: usize) -> usize {
    text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Byte offset of the end of the line containing `cursor` (just before `\n`).
fn line_end(text: &str, cursor: usize) -> usize {
    text[cursor..]
        .find('\n')
        .map(|i| cursor + i)
        .unwrap_or(text.len())
}

/// Move the cursor up (`delta = -1`) or down (`delta = 1`) one visual line,
/// preserving the display column when possible. Uses `char` counts so wide
/// glyphs behave predictably.
fn move_line(text: &str, cursor: usize, delta: i32) -> usize {
    let ls = line_start(text, cursor);
    let col = text[ls..cursor].chars().count();
    if delta < 0 {
        if ls == 0 {
            return cursor;
        }
        let prev_end = ls - 1; // the '\n' before this line
        let prev_start = line_start(text, prev_end);
        let prev_len = text[prev_start..prev_end].chars().count();
        let target = col.min(prev_len);
        char_index(&text[prev_start..], target)
            .map(|off| prev_start + off)
            .unwrap_or(prev_end)
    } else {
        let le = line_end(text, cursor);
        if le >= text.len() {
            return cursor;
        }
        let next_start = le + 1;
        let next_end = line_end(text, next_start);
        let next_len = text[next_start..next_end].chars().count();
        let target = col.min(next_len);
        char_index(&text[next_start..], target)
            .map(|off| next_start + off)
            .unwrap_or(next_end)
    }
}

/// Byte offset of the Nth character in `s` (or None if out of range).
fn char_index(s: &str, n: usize) -> Option<usize> {
    if n == 0 {
        return Some(0);
    }
    s.char_indices().nth(n).map(|(i, _)| i).or(Some(s.len()))
}

impl App {
    /// The message menu for the last thing the user sent.
    fn open_last_message_menu(&mut self) {
        if let Some(index) = self
            .blocks
            .iter()
            .rposition(|b| matches!(b.kind, TranscriptKind::User))
        {
            self.open_message_menu(index);
        } else {
            self.status = "no message to act on".into();
        }
    }

    /// Erase the character after the caret.
    fn delete_forward(&mut self) {
        if self.cursor >= self.input.len() {
            return;
        }
        let next = self.input[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.input.len());
        self.input.drain(self.cursor..next);
    }

    /// Ctrl+Backspace or Alt+Backspace: erase the word before the caret,
    /// with the spaces after it.
    fn delete_word_back(&mut self) {
        let before = &self.input[..self.cursor];
        let trimmed = before.trim_end_matches(|c: char| c.is_whitespace() && c != '\n');
        let start = trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(index, c)| index + c.len_utf8())
            .unwrap_or(0);
        // Directly after a newline, the newline itself goes, like one
        // Backspace would.
        let start = if start == self.cursor {
            before
                .char_indices()
                .last()
                .map(|(index, _)| index)
                .unwrap_or(0)
        } else {
            start
        };
        self.input.drain(start..self.cursor);
        self.cursor = start;
    }
}
