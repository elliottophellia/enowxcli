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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
    TypeSafe,
}

/// The sections of Settings, in the order they are listed.
pub(crate) const SECTIONS: [Page; 13] = [
    Page::General,
    Page::Models,
    Page::Providers,
    Page::Agents,
    Page::Team,
    Page::Mcp,
    Page::Rag,
    Page::Skills,
    Page::TypeSafe,
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
            Page::TypeSafe => "TypeSafe",
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
            Modal::Updates => Page::Updates,
            Modal::General => Page::General,
            Modal::Display => Page::Display,
            Modal::TypeSafe | Modal::TypeSafeKey => Page::TypeSafe,
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
        if self.active_tab == Tab::Settings {
            if self.modal == Modal::None {
                return SECTIONS[self.page_index.min(SECTIONS.len() - 1)];
            }
            if self.modal == Modal::SettingValue {
                if let Some(edit) = &self.setting_edit {
                    return edit.category;
                }
            }
            if self.modal == Modal::SettingsChoice {
                if let Some(choice) = &self.settings_choice {
                    return choice.category;
                }
            }
        }
        Page::of(self.modal)
    }

    pub(crate) fn tab(&self) -> Tab {
        self.active_tab
    }

    /// Open `page`: a section of Settings, or the chat.
    pub(crate) fn open_page(&mut self, page: Page) -> Result<()> {
        if let Some(index) = SECTIONS.iter().position(|candidate| *candidate == page) {
            if self.modal == Modal::Themes {
                self.theme = crate::theme::Theme::find(&self.config.ui.theme);
            }
            self.page_index = index;
            self.active_tab = Tab::Settings;
            self.modal = Modal::None;
            self.picker.events = None;
            self.picking_for_agent = None;
            self.modal_search.clear();
            self.modal_error.clear();
            self.settings_nav = false;
            self.settings_query.clear();
            self.settings_row_id = self
                .settings_rows()
                .iter()
                .find(|row| row.category == page)
                .map(|row| row.id.clone())
                .unwrap_or_default();
            self.chat_return = None;
        } else {
            if self.active_tab == Tab::Settings {
                let category = self.page();
                self.chat_return = Some(SettingsReturnContext {
                    category,
                    row_id: self.settings_row_id.clone(),
                    query: String::new(),
                });
            } else {
                self.chat_return = None;
            }
            self.active_tab = Tab::Chat;
            self.modal = Modal::None;
            self.picker.events = None;
            self.picking_for_agent = None;
            self.modal_search.clear();
            self.modal_error.clear();
            self.setting_edit = None;
            self.settings_choice = None;
            self.settings_nav = false;
            self.settings_query.clear();
        }
        Ok(())
    }

    /// Open a tab: Settings opens on the section used last, with the
    /// section list focused.
    pub(crate) fn open_tab(&mut self, tab: Tab) -> Result<()> {
        match tab {
            Tab::Chat => self.open_page(Page::Chat),
            Tab::Settings => {
                self.active_tab = Tab::Settings;
                if let Some(context) = self.chat_return.take() {
                    self.page_index = SECTIONS
                        .iter()
                        .position(|page| *page == context.category)
                        .unwrap_or(self.page_index);
                    self.settings_row_id = context.row_id;
                }
                self.settings_query.clear();
                self.setting_edit = None;
                self.settings_choice = None;
                self.settings_return = None;
                self.modal_error.clear();
                self.settings_nav = false;
                self.ensure_settings_selection();
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

    pub(crate) fn section_index(&self) -> usize {
        self.page_index.min(SECTIONS.len() - 1)
    }

    pub(crate) fn settings_page_key(&mut self, _key: &crossterm::event::KeyEvent) -> Result<bool> {
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
