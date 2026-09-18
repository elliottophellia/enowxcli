use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl App {
    /// React to an edit in the settings form.
    ///
    /// Editing the endpoint used to blank the API key, the model-list URL,
    /// the model and the context window — on every keystroke, before the user
    /// could save. Correcting a typo in a URL (adding a missing `/v1`, say)
    /// therefore cost the whole provider setup and forced a re-entry of
    /// everything.
    ///
    /// What actually stops being valid is scoped to the HOST: a key issued by
    /// one service is meaningless at another. That is decided at SAVE, in
    /// `reconcile_host_change`, not here — every prefix of a URL is typed on
    /// the way to the full one, and `https://a` is a perfectly valid host that
    /// happens to be one keystroke into `https://ai.example.id`. Clearing on
    /// each keystroke wiped the key before the user finished the word.
    ///
    /// The model list held in memory is still dropped, since it was fetched
    /// from the old endpoint and may no longer describe this one.
    pub(crate) fn settings_changed(&mut self, field: SettingsField) {
        self.modal_error.clear();
        if matches!(field, SettingsField::Provider | SettingsField::BaseUrl) {
            self.settings.preset = "custom".into();
        }
        if field != SettingsField::Model && field != SettingsField::ContextWindow {
            self.models.clear();
            self.modal_items.clear();
            self.model_events = None;
            self.discovering_models = false;
        }
    }

    pub(crate) fn settings_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => self.modal = Modal::None,
                KeyCode::Char('u') => {
                    self.settings_changed(SETTINGS_FIELDS[self.modal_cursor]);
                    self.settings
                        .value_mut(SETTINGS_FIELDS[self.modal_cursor])
                        .clear();
                    self.field_cursor = 0;
                }
                _ => {}
            }
            return Ok(());
        }
        let field = SETTINGS_FIELDS[self.modal_cursor];
        match key.code {
            KeyCode::Esc => {
                self.modal = Modal::None;
                self.model_events = None;
            }
            KeyCode::Up | KeyCode::BackTab if self.modal == Modal::Settings => {
                self.modal_cursor =
                    (self.modal_cursor + SETTINGS_FIELDS.len() - 1) % SETTINGS_FIELDS.len();
                self.field_cursor = self
                    .settings
                    .value(SETTINGS_FIELDS[self.modal_cursor])
                    .len();
            }
            KeyCode::Down | KeyCode::Tab if self.modal == Modal::Settings => {
                self.modal_cursor = (self.modal_cursor + 1) % SETTINGS_FIELDS.len();
                self.field_cursor = self
                    .settings
                    .value(SETTINGS_FIELDS[self.modal_cursor])
                    .len();
            }
            KeyCode::Enter if self.modal == Modal::ModelUrl => self.discover_models(),
            KeyCode::Enter if self.modal == Modal::ProviderKey => self.connect_preset()?,
            KeyCode::Enter if field == SettingsField::Theme => {
                self.open_themes();
                return Ok(());
            }
            KeyCode::F(2) if field == SettingsField::Theme => {
                self.open_themes();
                return Ok(());
            }
            KeyCode::Enter => self.save_settings()?,
            KeyCode::F(5) if self.modal == Modal::Settings => self.open_model_source(),
            KeyCode::Char(character) => {
                self.settings_changed(field);
                let value = self.settings.value_mut(field);
                value.insert(self.field_cursor, character);
                self.field_cursor += character.len_utf8();
            }
            KeyCode::Backspace if self.field_cursor > 0 => {
                self.settings_changed(field);
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
                self.settings_changed(field);
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
