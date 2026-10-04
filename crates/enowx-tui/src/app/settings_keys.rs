use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    /// The field the cursor is on in the open form.
    fn form_field(&self) -> Option<SettingsField> {
        self.current_form_fields().get(self.modal_cursor).copied()
    }

    /// React to an edit in a form: the error it showed no longer applies.
    ///
    /// Nothing else is cleared while typing. Editing the endpoint used to
    /// blank the key, the model-list URL and the model on every keystroke,
    /// so correcting a typo in a URL cost the whole setup. What stops being
    /// valid when the host changes is decided at save, against the finished
    /// URL (`save_provider_form`): every prefix of a URL is typed on the way
    /// to the full one.
    pub(crate) fn settings_changed(&mut self) {
        self.modal_error.clear();
    }

    /// Route a paste to wherever the user is typing.
    ///
    /// Lives here rather than in the runtime's event match so it can be
    /// tested: the branch that used to decide this listed the form modals by
    /// hand, and a form missing from that list silently dropped every paste.
    pub(crate) fn paste(&mut self, text: &str) {
        if self.modal == Modal::None && self.paste_into_question(text) {
            return;
        }
        if self.modal.is_form() {
            self.paste_into_form(text);
        } else if self.modal == Modal::None {
            let clean = crate::text::sanitize_paste(text);
            let cursor = self.cursor.min(self.input.len());
            self.input.insert_str(cursor, &clean);
            self.cursor = cursor + clean.len();
        } else if self.modal == Modal::Models {
            // The model list filters by what is typed, pasted or not.
            let clean: String = text.chars().filter(|c| !c.is_control()).collect();
            self.search_models(|search| search.push_str(clean.trim()));
        }
        // Any other modal is a list, with nothing to paste into.
    }

    /// Insert pasted text into whichever form field is being edited.
    ///
    /// The MCP form keeps its own fields rather than the provider forms', so
    /// routing every form's paste through the settings draft would write a
    /// pasted command into the provider name.
    pub(crate) fn paste_into_form(&mut self, text: &str) {
        // A pasted key or URL is a single line; control characters in it are
        // either terminal noise or an attempt to move the cursor mid-paste.
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if text.is_empty() {
            return;
        }
        if self.modal == Modal::McpForm {
            // Transport is a cycled choice, not a text field: pasting into it
            // would write into whatever slot its accessor happens to borrow.
            if crate::modal::MCP_FORM_FIELDS[self.mcp_field]
                == crate::modal::McpFormField::Transport
            {
                return;
            }
            // This form appends as you type rather than tracking a cursor, so
            // a paste lands the same way.
            self.mcp_field_mut().push_str(&text);
            return;
        }
        let Some(field) = self.form_field() else {
            return;
        };
        self.settings_changed();
        let cursor = self.field_cursor.min(self.settings.value(field).len());
        self.settings.value_mut(field).insert_str(cursor, &text);
        self.field_cursor = cursor + text.len();
    }

    pub(crate) fn settings_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(field) = self.form_field() else {
            self.modal = Modal::None;
            return Ok(());
        };
        let fields = self.current_form_fields().len();
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.modal = Modal::None,
                KeyCode::Char('u') => {
                    self.settings_changed();
                    self.settings.value_mut(field).clear();
                    self.field_cursor = 0;
                }
                _ => {}
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Esc => {
                // Back to the list the form was opened from.
                match self.modal {
                    Modal::ProviderKey | Modal::ProviderForm => {
                        let id = self.settings.provider_id.clone();
                        self.refresh_providers();
                        // On the provider the form was for, or on the row
                        // that adds one.
                        self.modal_cursor = self
                            .provider_ids
                            .iter()
                            .position(|p| *p == id)
                            .unwrap_or(self.provider_ids.len());
                    }
                    Modal::ModelManual | Modal::ModelEdit => {
                        self.modal = Modal::Models;
                        self.modal_cursor = 0;
                        self.move_picker(0);
                    }
                    _ => self.modal = Modal::None,
                }
                self.settings.api_key.clear();
            }
            KeyCode::Up | KeyCode::BackTab if fields > 1 => {
                self.modal_cursor = (self.modal_cursor + fields - 1) % fields;
                self.field_cursor = self
                    .settings
                    .value(self.form_field().unwrap_or(field))
                    .len();
            }
            KeyCode::Down | KeyCode::Tab if fields > 1 => {
                self.modal_cursor = (self.modal_cursor + 1) % fields;
                self.field_cursor = self
                    .settings
                    .value(self.form_field().unwrap_or(field))
                    .len();
            }
            KeyCode::Enter => match self.modal {
                Modal::ProviderKey => self.connect_key()?,
                Modal::ProviderForm => self.save_provider_form()?,
                Modal::ModelManual => self.save_manual_model()?,
                Modal::ModelEdit => self.save_edit_model()?,
                Modal::BuiltinMcp => self.save_builtin_mcp()?,
                Modal::Rag => self.save_rag()?,
                Modal::Team => self.save_team()?,
                Modal::Decision => self.save_decision()?,
                Modal::AcpEngine => self.save_acp_engine()?,
                Modal::AcpCustom => self.save_acp_custom()?,
                Modal::Updates => self.save_updates()?,
                Modal::General | Modal::Display => self.save_prefs()?,
                _ => {}
            },
            // A cycled choice (effort, vision) steps with Left/Right or
            // Space; it has no text to type into.
            KeyCode::Left if field.is_choice() => {
                self.settings_changed();
                self.settings.cycle_choice(field, -1);
            }
            KeyCode::Right | KeyCode::Char(' ') if field.is_choice() => {
                self.settings_changed();
                self.settings.cycle_choice(field, 1);
            }
            KeyCode::Char(_) if field.is_choice() => {}
            KeyCode::Char(character) => {
                self.settings_changed();
                let value = self.settings.value_mut(field);
                value.insert(self.field_cursor, character);
                self.field_cursor += character.len_utf8();
            }
            KeyCode::Backspace if field.is_choice() => {}
            KeyCode::Backspace if self.field_cursor > 0 => {
                self.settings_changed();
                let value = self.settings.value_mut(field);
                let previous = value[..self.field_cursor]
                    .char_indices()
                    .last()
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                value.drain(previous..self.field_cursor);
                self.field_cursor = previous;
            }
            KeyCode::Delete => {
                self.settings_changed();
                let value = self.settings.value_mut(field);
                if self.field_cursor < value.len() {
                    let next = value[self.field_cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(index, _)| self.field_cursor + index)
                        .unwrap_or(value.len());
                    value.drain(self.field_cursor..next);
                }
            }
            KeyCode::Left if self.field_cursor > 0 => {
                self.field_cursor = self.settings.value(field)[..self.field_cursor]
                    .char_indices()
                    .last()
                    .map(|(index, _)| index)
                    .unwrap_or(0);
            }
            KeyCode::Right => {
                let value = self.settings.value(field);
                if self.field_cursor < value.len() {
                    self.field_cursor = value[self.field_cursor..]
                        .char_indices()
                        .nth(1)
                        .map(|(index, _)| self.field_cursor + index)
                        .unwrap_or(value.len());
                }
            }
            KeyCode::Home => self.field_cursor = 0,
            KeyCode::End => self.field_cursor = self.settings.value(field).len(),
            _ => {}
        }
        Ok(())
    }
}

/// Scheme and authority of a URL (`https://host:port`), lowercased, or None
/// when the text is not yet a usable URL — the state every partially typed
/// URL passes through, and the reason this returns an Option rather than
/// guessing.
///
/// Hand-rolled rather than pulling an HTTP client into the UI crate: the only
/// question here is whether two endpoints point at the same host.
pub(crate) fn origin_of(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    // Authority runs to the first `/`, `?` or `#`.
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    // Strip any userinfo; the host is what identifies the service.
    let authority = authority.rsplit('@').next().unwrap_or_default();
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}

#[cfg(test)]
mod origin_tests {
    use super::origin_of;

    #[test]
    fn paths_do_not_change_the_origin() {
        assert_eq!(
            origin_of("https://ai.example.id/v1"),
            origin_of("https://ai.example.id/v2/chat")
        );
    }

    #[test]
    fn a_different_host_is_a_different_origin() {
        assert_ne!(
            origin_of("https://ai.example.id/v1"),
            origin_of("https://other.example.id/v1")
        );
    }

    #[test]
    fn a_port_is_part_of_the_origin() {
        assert_ne!(
            origin_of("http://localhost:8080/v1"),
            origin_of("http://localhost:9090/v1")
        );
    }

    #[test]
    fn case_is_ignored_in_the_host() {
        assert_eq!(
            origin_of("https://AI.Example.ID/v1"),
            origin_of("https://ai.example.id/v1")
        );
    }

    /// Every prefix of a URL is typed on the way to the full one; none of
    /// them may read as a move to a new host.
    #[test]
    fn partial_input_has_no_origin() {
        for partial in ["h", "https", "https:/", "https://", "https://@", "  "] {
            assert_eq!(origin_of(partial), None, "{partial:?} should not parse");
        }
    }

    #[test]
    fn userinfo_is_not_part_of_the_identity() {
        assert_eq!(
            origin_of("https://user:pw@ai.example.id/v1"),
            origin_of("https://ai.example.id/v1")
        );
    }
}
