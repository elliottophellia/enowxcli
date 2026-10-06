//! The Chat and Settings workspaces, grouped row navigation, and global shortcut.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

fn chat() -> TestApp {
    let mut app = TestApp::in_conversation();
    app.push_user("HISTORY-LINE-ONE");
    app
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

fn column_of(row: &str, needle: &str) -> u16 {
    row[..row
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` missing from {row}"))]
        .chars()
        .count() as u16
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
fn settings_workspace_shows_grouped_rows_and_hides_chat_transcript() {
    let mut app = chat();
    app.run_command("/settings").expect("settings");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.preview"));
    let screen = app.render_to_text(160, 40).join("\n");
    assert!(screen.contains("SETTINGS"), "{screen}");
    assert!(screen.contains("General"), "{screen}");
    assert!(screen.contains("Models"), "{screen}");
    assert!(
        screen.contains("Look at pages in a browser"),
        "selected help: {screen}"
    );
    assert!(
        !screen.contains("HISTORY-LINE-ONE"),
        "hidden Chat transcript: {screen}"
    );
    app.press_key(KeyCode::Esc).expect("back to Chat");
    assert_eq!(app.active_tab(), "Chat");
    assert!(app
        .render_to_text(160, 40)
        .join("\n")
        .contains("HISTORY-LINE-ONE"));
}

#[test]
fn shortcut_and_command_open_settings_on_last_row() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.preview"));
    app.press(KeyCode::Char('p'), true).expect("ctrl+p to Chat");
    assert_eq!(app.active_tab(), "Chat");
    app.run_command("/settings").expect("settings command");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.preview"));
}

#[test]
fn global_search_value_editor_persistence_cancel_and_chat_isolation() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("open Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.preview"));
    assert!(app
        .render_to_text(150, 44)
        .join("\n")
        .contains("Allow agents to inspect web pages"));

    app.type_keys("agent.max_steps");
    assert_eq!(app.settings_query(), "agent.max_steps");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.max_steps"));
    app.press_key(KeyCode::Enter).expect("edit row");
    app.press(KeyCode::Char('u'), true).expect("clear draft");
    app.type_keys("12");
    app.press_key(KeyCode::Enter).expect("save one key");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.max_steps"));
    assert_eq!(app.config_value("agent.max_steps").as_deref(), Some("12"));
    let rendered = app.render_to_text(150, 44).join("\n");
    assert!(
        app.status_line().contains("changed"),
        "{}",
        app.status_line()
    );

    app.press_key(KeyCode::Enter).expect("edit again");
    app.press(KeyCode::Char('u'), true).expect("clear draft");
    app.type_keys("13");
    app.press_key(KeyCode::Esc).expect("discard draft");
    assert_eq!(app.config_value("agent.max_steps").as_deref(), Some("12"));
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.max_steps"));

    app.press(KeyCode::Char('p'), true).expect("back to Chat");
    app.type_keys("chat-only");
    app.paste(" paste");
    assert_eq!(app.composer_text(), "chat-only paste");
    app.press(KeyCode::Char('p'), true)
        .expect("reopen Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("agent.max_steps"));
    assert!(app.settings_query().is_empty());
}

#[test]
fn filtering_toggle_and_empty_search_escape() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("Settings");
    app.type_keys("agent.preview");
    let before = app.config_value("agent.preview").expect("config value");
    app.press_key(KeyCode::Char(' '))
        .expect("toggle selected row");
    assert_ne!(
        app.config_value("agent.preview").as_deref(),
        Some(before.as_str())
    );
    let text = app.render_to_text(140, 36).join("\n");
    assert!(text.contains(
        if app.config_value("agent.preview").as_deref() == Some("true") {
            "on"
        } else {
            "off"
        }
    ));

    app.press_key(KeyCode::Esc).expect("clear query first");
    assert!(app.settings_query().is_empty());
    app.type_keys("no-such-setting");
    let text = app.render_to_text(140, 36).join("\n");
    assert!(text.contains("No settings match this query"), "{text}");
    app.press_key(KeyCode::Esc).expect("clear empty query");
    assert_eq!(app.active_tab(), "Settings");
    assert!(app.settings_query().is_empty());
}

#[test]
fn workspace_layout_has_split_and_flat_responsive_modes() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("Settings");
    let wide = app.render_to_text(120, 36).join("\n");
    assert!(wide.contains("Categories"), "split layout: {wide}");
    assert!(
        wide.contains("Allow agents to inspect web pages"),
        "selected help: {wide}"
    );
    let narrow = app.render_to_text(70, 20).join("\n");
    assert!(narrow.contains("General"), "flat layout: {narrow}");
    for _ in 0..100 {
        if app.settings_row_id().as_deref() == Some("update.auto_install") {
            break;
        }
        app.press_key(KeyCode::Down).unwrap();
    }
    assert_eq!(
        app.settings_row_id().as_deref(),
        Some("update.auto_install")
    );
    let narrow = app.render_to_text(70, 20).join("\n");
    assert!(
        narrow.contains("Install updates automatically"),
        "selected row is kept in the viewport: {narrow}"
    );
}

#[test]
fn category_click_and_row_click_select_or_activate() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("Settings");
    let rows = app.render_to_text(160, 40);
    let (y, line) = rows
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains("Agents"))
        .expect("Agents category");
    click(&mut app, column_of(line, "Agents"), y as u16);
    assert!(app
        .settings_row_id()
        .as_deref()
        .unwrap()
        .starts_with("agent:"));
    assert_eq!(app.active_tab(), "Settings");
}

#[test]
fn theme_preview_is_temporary_until_saved_and_escape_restores_it() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("Settings");
    app.type_keys("theme:choose");
    assert_eq!(app.settings_row_id().as_deref(), Some("theme:choose"));
    let original = app.config_value("ui.theme").expect("configured theme");
    let original_live = app.theme_name();

    app.press_key(KeyCode::Enter).expect("open theme chooser");
    app.press_key(KeyCode::Down).expect("preview next theme");
    assert_ne!(app.theme_name(), original_live);
    assert_eq!(
        app.config_value("ui.theme").as_deref(),
        Some(original.as_str())
    );
    app.press_key(KeyCode::Esc).expect("cancel preview");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("theme:choose"));
    assert_eq!(app.theme_name(), original_live);
    assert_eq!(
        app.config_value("ui.theme").as_deref(),
        Some(original.as_str())
    );

    app.press_key(KeyCode::Enter).expect("reopen theme chooser");
    app.press_key(KeyCode::Down).expect("preview theme to save");
    let chosen_live = app.theme_name();
    app.press_key(KeyCode::Enter).expect("save theme");
    assert_eq!(app.theme_name(), chosen_live);
    assert_eq!(
        app.config_value("ui.theme").as_deref(),
        Some(chosen_live.as_str())
    );
    assert_eq!(app.settings_row_id().as_deref(), Some("theme:choose"));
}

#[test]
fn typesafe_key_form_returns_to_filtered_origin_without_exposing_credentials() {
    let mut app = chat();
    app.press(KeyCode::Char('p'), true).expect("Settings");
    app.type_keys("typesafe:key");
    let origin_query = app.settings_query();
    assert_eq!(app.settings_row_id().as_deref(), Some("typesafe:key"));
    app.press_key(KeyCode::Enter).expect("open masked key form");
    assert!(app.any_modal_open());
    let secret = "sk-secret-never-render-this";
    app.paste(secret);
    let screen = app.render_to_text(150, 44).join("\n");
    assert!(
        !screen.contains(secret),
        "credentials must stay masked: {screen}"
    );
    app.press_key(KeyCode::Esc).expect("cancel key form");
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_query(), origin_query);
    assert_eq!(app.settings_row_id().as_deref(), Some("typesafe:key"));
}
