//! The tabs at the top right: Chat, and a page for each kind of setting.
//!
//! A settings page is the modal that already edits that setting, drawn over
//! the whole main column instead of as a popup, so the chat gives its room
//! to the page while it is open. Every key and click the modal handled before
//! still works; `Ctrl+P` moves to the next tab, a click on a tab opens it,
//! and `Esc` comes back to the chat.

use super::*;

/// One tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Chat,
    Models,
    Providers,
    Agents,
    Mcp,
    Skills,
    Sessions,
    Theme,
}

/// The tabs, in the order `Ctrl+P` steps through them.
pub(crate) const PAGES: [Page; 8] = [
    Page::Chat,
    Page::Models,
    Page::Providers,
    Page::Agents,
    Page::Mcp,
    Page::Skills,
    Page::Sessions,
    Page::Theme,
];

impl Page {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Page::Chat => "Chat",
            Page::Models => "Models",
            Page::Providers => "Providers",
            Page::Agents => "Agents",
            Page::Mcp => "MCP",
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
            Modal::Skills => Page::Skills,
            Modal::Sessions => Page::Sessions,
            Modal::Themes => Page::Theme,
            _ => Page::Chat,
        }
    }
}

impl App {
    /// The tab on screen.
    pub(crate) fn page(&self) -> Page {
        Page::of(self.modal)
    }

    /// Open `page`: its modal, drawn full size, or the chat.
    pub(crate) fn open_page(&mut self, page: Page) -> Result<()> {
        self.page_index = PAGES.iter().position(|p| *p == page).unwrap_or(0);
        // The theme page previews as you move; leaving it without choosing
        // puts the saved theme back, or the preview sticks.
        if self.modal == Modal::Themes {
            self.theme = crate::theme::Theme::find(&self.config.ui.theme);
        }
        // Leaving a page closes its modal the way Esc does, so nothing it
        // was holding (a model list being fetched, an agent being given a
        // model) carries over to the next one.
        self.modal = Modal::None;
        self.picker.events = None;
        self.picking_for_agent = None;
        self.modal_search.clear();
        self.modal_error.clear();
        match page {
            Page::Chat => {}
            Page::Models => self.open_model_picker(None),
            Page::Providers => self.open_providers(),
            Page::Agents => self.open_agents(),
            Page::Mcp => self.open_mcp(),
            Page::Skills => self.open_skills(),
            Page::Sessions => self.open_sessions()?,
            Page::Theme => self.open_themes(),
        }
        Ok(())
    }

    /// `Ctrl+P`: the next tab, after the last one the chat again.
    ///
    /// Counted from the tab last chosen, not from the modal on screen: a page
    /// can open another (Models with no provider connected opens Providers),
    /// and stepping from that one skipped tabs.
    pub(crate) fn next_page(&mut self) -> Result<()> {
        let at = if self.modal == Modal::None {
            0
        } else {
            self.page_index
        };
        let next = PAGES[(at + 1) % PAGES.len()];
        self.open_page(next)
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
