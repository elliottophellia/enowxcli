//! The two tabs at the top right: Chat and Settings.
//!
//! Settings replaces the chat in the main column with a list of sections on
//! the left (Models, Providers, Agents, MCP, Skills, Sessions, Theme) and the
//! chosen section beside it. A section is the modal that already edits that
//! setting, drawn in place instead of as a popup, so every key and click it
//! handled before still works.
//!
//! Keys: `Ctrl+P` switches between Chat and Settings. Settings opens with
//! the section list focused: `Up`/`Down` pick a section (shown beside it),
//! `Enter` goes into it. `Esc` goes back one level: from a form to its list,
//! from a section to the section list, from the section list to the chat.
//! `Left` also steps out to the list. The mouse does the same: a click on a
//! section picks it, a click in the section goes into it.

use super::*;

/// A section of Settings, or the chat.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Chat,
    Models,
    Providers,
    Agents,
    Mcp,
    Rag,
    Team,
    Skills,
    Sessions,
    Theme,
    Updates,
    General,
    Display,
    Decision,
    Acp,
}

/// The sections of Settings, in the order they are listed.
pub(crate) const SECTIONS: [Page; 14] = [
    Page::General,
    Page::Models,
    Page::Providers,
    Page::Agents,
    Page::Acp,
    Page::Team,
    Page::Mcp,
    Page::Rag,
    Page::Skills,
    Page::Decision,
    Page::Sessions,
    Page::Display,
    Page::Theme,
    Page::Updates,
];

/// The tabs at the top right.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tab {
    Chat,
    Settings,
}

impl Tab {
    pub(crate) const ALL: [Tab; 2] = [Tab::Chat, Tab::Settings];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Chat => "Chat",
            Tab::Settings => "Settings",
        }
    }
}

impl Page {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Page::Chat => "Chat",
            Page::Models => "Models",
            Page::Providers => "Providers",
            Page::Agents => "Agents",
            Page::Mcp => "MCP",
            Page::Rag => "RAG",
            Page::Team => "Team",
            Page::Updates => "Updates",
            Page::General => "General",
            Page::Display => "Display",
            Page::Decision => "Decision model",
            Page::Acp => "ACP agents",
            Page::Skills => "Skills",
            Page::Sessions => "Sessions",
            Page::Theme => "Theme",
        }
    }

    /// The page a modal belongs to. A form opened from a page (a provider's
    /// key, a model's properties, an MCP server's setup) stays on its page;
    /// anything else is not a page and draws as a popup over the chat.
    pub(crate) fn of(modal: Modal) -> Page {
        match modal {
            Modal::Models | Modal::ModelManual | Modal::ModelEdit | Modal::Effort => Page::Models,
            Modal::Providers | Modal::ProviderKey | Modal::ProviderForm => Page::Providers,
            Modal::Agents => Page::Agents,
            Modal::Mcp | Modal::McpForm | Modal::BuiltinMcp => Page::Mcp,
            Modal::Rag => Page::Rag,
            Modal::Team => Page::Team,
            Modal::Decision => Page::Decision,
            Modal::Acp | Modal::AcpEngine | Modal::AcpCustom => Page::Acp,
            Modal::Updates => Page::Updates,
            Modal::General => Page::General,
            Modal::Display => Page::Display,
            Modal::Skills => Page::Skills,
            Modal::Sessions => Page::Sessions,
            Modal::Themes => Page::Theme,
            _ => Page::Chat,
        }
    }
}

impl App {
    /// The section on screen, or `Chat`.
    pub(crate) fn page(&self) -> Page {
        Page::of(self.modal)
    }

    /// The tab on screen.
    pub(crate) fn tab(&self) -> Tab {
        if self.page() == Page::Chat {
            Tab::Chat
        } else {
            Tab::Settings
        }
    }

    /// Open `page`: a section of Settings, or the chat.
    pub(crate) fn open_page(&mut self, page: Page) -> Result<()> {
        // Remembered for the next time Settings opens, however the section
        // was reached (a tab, the list, or `/skills` typed in the chat).
        if let Some(index) = SECTIONS.iter().position(|p| *p == self.page()) {
            self.page_index = index;
        }
        if let Some(index) = SECTIONS.iter().position(|p| *p == page) {
            self.page_index = index;
        }
        // The theme section previews as you move; leaving it without
        // choosing puts the saved theme back, or the preview sticks.
        if self.modal == Modal::Themes {
            self.theme = crate::theme::Theme::find(&self.config.ui.theme);
        }
        // Leaving a section closes its modal the way Esc does, so nothing it
        // was holding (a model list being fetched, an agent being given a
        // model) carries over to the next one.
        self.modal = Modal::None;
        self.picker.events = None;
        self.picking_for_agent = None;
        self.modal_search.clear();
        self.modal_error.clear();
        self.settings_nav = false;
        match page {
            Page::Chat => {}
            Page::Models => self.open_model_picker(None),
            Page::Providers => self.open_providers(),
            Page::Agents => self.open_agents(),
            Page::Mcp => self.open_mcp(),
            Page::Rag => self.open_rag(),
            Page::Team => self.open_team(),
            Page::Updates => self.open_updates(),
            Page::General => self.open_prefs(Modal::General),
            Page::Display => self.open_prefs(Modal::Display),
            Page::Decision => self.open_decision(),
            Page::Acp => self.open_acp(),
            Page::Skills => self.open_skills(),
            Page::Sessions => self.open_sessions()?,
            Page::Theme => self.open_themes(),
        }
        Ok(())
    }

    /// Open a tab: Settings opens on the section used last, with the
    /// section list focused.
    pub(crate) fn open_tab(&mut self, tab: Tab) -> Result<()> {
        match tab {
            Tab::Chat => self.open_page(Page::Chat),
            Tab::Settings if self.tab() == Tab::Settings => {
                self.settings_nav = true;
                Ok(())
            }
            Tab::Settings => {
                self.open_page(SECTIONS[self.page_index.min(SECTIONS.len() - 1)])?;
                self.settings_nav = true;
                Ok(())
            }
        }
    }

    /// `Ctrl+P`: Chat to Settings and back.
    pub(crate) fn switch_tab(&mut self) -> Result<()> {
        match self.tab() {
            Tab::Chat => self.open_tab(Tab::Settings),
            Tab::Settings => self.open_tab(Tab::Chat),
        }
    }

    /// The section list's place in `SECTIONS`: the section on screen, which
    /// may differ from the one chosen (Models with no provider connected
    /// opens Providers).
    pub(crate) fn section_index(&self) -> usize {
        SECTIONS
            .iter()
            .position(|p| *p == self.page())
            .unwrap_or(self.page_index)
    }

    /// Esc inside a section: one level back. A section's own list or form
    /// gives the focus to the section list; a form opened from that list
    /// (a provider's key, an MCP server's setup) goes back to the list; a
    /// search being typed is cleared first.
    fn settings_escape(&mut self) -> Result<bool> {
        match self.modal {
            Modal::Models if !self.modal_search.is_empty() || self.picking_for_agent.is_some() => {
                Ok(false)
            }
            Modal::AcpEngine | Modal::AcpCustom => {
                let id = self.settings.provider_id.clone();
                self.modal_error.clear();
                self.open_acp();
                self.modal_cursor = self
                    .modal_items
                    .iter()
                    .position(|(row, _)| *row == id)
                    .unwrap_or(0);
                Ok(true)
            }
            Modal::McpForm | Modal::BuiltinMcp => {
                let at = self.modal_cursor;
                self.modal_error.clear();
                self.open_mcp();
                self.modal_cursor = at.min(self.mcp_rows().len().saturating_sub(1));
                Ok(true)
            }
            Modal::Models
            | Modal::Providers
            | Modal::Agents
            | Modal::Mcp
            | Modal::Rag
            | Modal::Team
            | Modal::Decision
            | Modal::Acp
            | Modal::Updates
            | Modal::General
            | Modal::Display
            | Modal::Skills
            | Modal::Sessions
            | Modal::Themes
            | Modal::Effort => {
                // The theme section previews as you move: stepping out
                // without choosing puts the saved theme back.
                if self.modal == Modal::Themes {
                    self.theme = crate::theme::Theme::find(&self.config.ui.theme);
                }
                self.settings_nav = true;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Keys while Settings is on screen, before the section sees them.
    /// Returns whether the key was taken.
    pub(crate) fn settings_page_key(&mut self, key: &crossterm::event::KeyEvent) -> Result<bool> {
        use crossterm::event::{KeyCode, KeyModifiers};
        if self.tab() != Tab::Settings {
            self.settings_nav = false;
            return Ok(false);
        }
        // Esc from a section closes it in the section's own handler; the
        // section is remembered here so Settings opens on it again.
        self.page_index = self.section_index();
        if self.settings_nav {
            match key.code {
                KeyCode::Up | KeyCode::Down => {
                    let at = self.section_index();
                    let next = if key.code == KeyCode::Up {
                        at.checked_sub(1).unwrap_or(SECTIONS.len() - 1)
                    } else {
                        (at + 1) % SECTIONS.len()
                    };
                    self.open_page(SECTIONS[next])?;
                    self.settings_nav = true;
                }
                KeyCode::Right | KeyCode::Enter | KeyCode::Tab => self.settings_nav = false,
                KeyCode::Esc => self.open_page(Page::Chat)?,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.open_page(Page::Chat)?
                }
                _ => {}
            }
            return Ok(true);
        }
        if key.code == KeyCode::Esc {
            return self.settings_escape();
        }
        // Left steps out to the section list from a section's list. A form
        // keeps it for its caret and its choices, unless the caret is
        // already at the start of a text field.
        let at_start = self.modal.is_form()
            && self.modal != Modal::McpForm
            && self.field_cursor == 0
            && self
                .current_form_fields()
                .get(self.modal_cursor)
                .is_some_and(|f| !f.is_choice());
        if key.code == KeyCode::Left
            && key.modifiers.is_empty()
            && (!self.modal.is_form() || at_start)
        {
            self.settings_nav = true;
            return Ok(true);
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_stays_on_the_page_it_was_opened_from() {
        assert_eq!(Page::of(Modal::ProviderKey), Page::Providers);
        assert_eq!(Page::of(Modal::ModelEdit), Page::Models);
        assert_eq!(Page::of(Modal::BuiltinMcp), Page::Mcp);
        assert_eq!(Page::of(Modal::QuitConfirm), Page::Chat);
        assert_eq!(Page::of(Modal::None), Page::Chat);
    }
}
