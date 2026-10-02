//! The tabs at the top right: Chat, and a page per kind of setting. A page
//! takes the main column in place of the chat; Ctrl+P steps through the tabs,
//! a click opens one, and Esc comes back to the chat.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

fn chat() -> TestApp {
    let mut app = TestApp::in_conversation();
    app.push_user("HISTORY-LINE-ONE");
    app
}

#[test]
fn the_tabs_sit_at_the_top_right() {
    let mut app = chat();
    let rows = app.render_to_text(160, 40);
    let top = &rows[0];
    for tab in [
        "Chat",
        "Models",
        "Providers",
        "Agents",
        "MCP",
        "Skills",
        "Sessions",
        "Theme",
    ] {
        assert!(top.contains(tab), "`{tab}` in the top row: {top}");
    }
    // Right side: the tabs end near the right edge of the main column.
    assert!(top.find("Theme").unwrap() > 60, "{top}");
}

#[test]
fn a_page_replaces_the_chat_and_esc_brings_it_back() {
    let mut app = chat();
    assert!(app
        .render_to_text(160, 40)
        .join("\n")
        .contains("HISTORY-LINE-ONE"));
    app.run_command("/mcp").expect("mcp page");
    let page = app.render_to_text(160, 40).join("\n");
    assert!(page.contains("coolify"), "{page}");
    assert!(
        !page.contains("HISTORY-LINE-ONE"),
        "the chat gives way to the page: {page}"
    );
    app.press_key(KeyCode::Esc).expect("esc");
    assert!(!app.is_modal_open());
    assert!(app
        .render_to_text(160, 40)
        .join("\n")
        .contains("HISTORY-LINE-ONE"));
}

#[test]
fn ctrl_p_steps_through_the_tabs_and_back_to_the_chat() {
    let mut app = chat();
    let mut titles = Vec::new();
    for _ in 0..8 {
        app.press(KeyCode::Char('p'), true).expect("ctrl+p");
        titles.push(app.modal_title().trim().to_owned());
    }
    // Models opens first; after the last tab the chat comes back (no modal).
    assert!(
        app.render_to_text(160, 40)
            .join("\n")
            .contains("HISTORY-LINE-ONE"),
        "{titles:?}"
    );
    assert!(!app.is_modal_open(), "back on the chat: {titles:?}");
    assert!(titles.iter().any(|t| t.contains("MCP")), "{titles:?}");
    assert!(titles.iter().any(|t| t.contains("THEME")), "{titles:?}");
}

/// The screen column of `text` in `row`: characters, not bytes, since the
/// box edges are multi-byte.
fn column_of(row: &str, text: &str) -> u16 {
    let byte = row
        .find(text)
        .unwrap_or_else(|| panic!("`{text}` in {row}"));
    row[..byte].chars().count() as u16
}

#[test]
fn a_click_on_a_tab_opens_its_page() {
    let mut app = chat();
    let rows = app.render_to_text(160, 40);
    let col = column_of(&rows[0], "Skills");
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col + 1,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
    assert!(
        app.modal_title().contains("SKILLS"),
        "{}",
        app.modal_title()
    );
    // And the Chat tab closes it again.
    let rows = app.render_to_text(160, 40);
    let col = column_of(&rows[0], " Chat ");
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col + 2,
        row: 0,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
    assert!(!app.is_modal_open());
}
