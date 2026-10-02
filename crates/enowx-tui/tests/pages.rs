//! The tabs at the top right: Chat and Settings. Settings takes the main
//! column in place of the chat, with its sections listed on the left; Ctrl+P
//! switches tabs, a click opens a tab or a section, the arrows move between
//! the list and the section, and Esc comes back to the chat.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

fn chat() -> TestApp {
    let mut app = TestApp::in_conversation();
    app.push_user("HISTORY-LINE-ONE");
    app
}

#[test]
fn two_tabs_sit_at_the_top_right() {
    let mut app = chat();
    let rows = app.render_to_text(160, 40);
    let top = &rows[0];
    assert!(top.contains(" Chat "), "{top}");
    assert!(top.contains(" Settings "), "{top}");
    assert!(!top.contains("Providers"), "sections are not tabs: {top}");
    assert!(top.find("Settings").unwrap() > 100, "{top}");
}

#[test]
fn settings_replaces_the_chat_with_sections_beside_the_section() {
    let mut app = chat();
    app.run_command("/mcp").expect("mcp");
    let screen = app.render_to_text(160, 40).join("\n");
    assert!(screen.contains("SETTINGS"), "{screen}");
    for section in ["Models", "Providers", "Agents", "MCP", "Skills", "Sessions", "Theme"] {
        assert!(screen.contains(section), "`{section}` listed: {screen}");
    }
    assert!(screen.contains("coolify"), "the section beside it: {screen}");
    assert!(!screen.contains("HISTORY-LINE-ONE"), "{screen}");
    app.press_key(KeyCode::Esc).expect("esc");
    assert!(!app.is_modal_open());
    assert!(app
        .render_to_text(160, 40)
        .join("\n")
        .contains("HISTORY-LINE-ONE"));
}

#[test]
fn ctrl_p_switches_between_chat_and_settings() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert!(app.is_modal_open(), "settings open");
    let first = app.modal_title();
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert!(!app.is_modal_open(), "back on the chat");
    // Settings opens again on the section used last.
    app.run_command("/skills").unwrap();
    app.press(KeyCode::Char('p'), true).unwrap();
    app.press(KeyCode::Char('p'), true).unwrap();
    assert!(app.modal_title().contains("SKILLS"), "{first} then {}", app.modal_title());
}

#[test]
fn the_arrows_walk_the_section_list() {
    let mut app = chat();
    app.run_command("/agent").unwrap();
    assert!(app.modal_title().contains("AGENT"), "{}", app.modal_title());
    app.press_key(KeyCode::Left).unwrap();
    app.press_key(KeyCode::Down).unwrap();
    assert!(app.modal_title().contains("MCP"), "{}", app.modal_title());
    app.press_key(KeyCode::Up).unwrap();
    app.press_key(KeyCode::Up).unwrap();
    assert!(app.modal_title().contains("PROVIDER"), "{}", app.modal_title());
    // Right goes back into the section: Down moves its list, not the sections.
    app.press_key(KeyCode::Right).unwrap();
    app.press_key(KeyCode::Down).unwrap();
    assert!(app.modal_title().contains("PROVIDER"), "{}", app.modal_title());
    // Esc from the list goes back to the chat.
    app.press_key(KeyCode::Left).unwrap();
    app.press_key(KeyCode::Esc).unwrap();
    assert!(!app.is_modal_open());
}

/// The screen column of `text` in `row`: characters, not bytes, since the
/// box edges are multi-byte.
fn column_of(row: &str, text: &str) -> u16 {
    let byte = row
        .find(text)
        .unwrap_or_else(|| panic!("`{text}` in {row}"));
    row[..byte].chars().count() as u16
}

fn click(app: &mut TestApp, column: u16, row: u16) {
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
}

#[test]
fn clicks_open_a_tab_and_a_section() {
    let mut app = chat();
    let rows = app.render_to_text(160, 40);
    click(&mut app, column_of(&rows[0], " Settings ") + 2, 0);
    assert!(app.is_modal_open(), "settings open");
    let rows = app.render_to_text(160, 40);
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| r.contains(" Skills "))
        .expect("Skills listed");
    click(&mut app, column_of(row, "Skills"), y as u16);
    assert!(app.modal_title().contains("SKILLS"), "{}", app.modal_title());
    let rows = app.render_to_text(160, 40);
    click(&mut app, column_of(&rows[0], " Chat ") + 2, 0);
    assert!(!app.is_modal_open());
}

#[test]
fn settings_reopens_on_the_section_left_with_esc() {
    let mut app = chat();
    app.run_command("/theme").unwrap();
    app.press_key(KeyCode::Esc).unwrap();
    assert!(!app.is_modal_open());
    app.press(KeyCode::Char('p'), true).unwrap();
    assert!(app.modal_title().contains("THEME"), "{}", app.modal_title());
}
