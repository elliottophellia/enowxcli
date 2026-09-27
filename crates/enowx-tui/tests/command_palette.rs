//! Ctrl+P opens the command palette: a floating, searchable list.
//!
//! Typing `/` still shows the inline list above the composer. That serves
//! someone who knows the name they want; this is for looking.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn open(app: &mut TestApp) {
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    // Every test below asserts against an OPEN palette. Without this, a
    // binding that does nothing leaves the ones asserting `!palette_open()`
    // passing for the wrong reason.
    assert!(app.palette_open(), "ctrl+p should have opened the palette");
}

#[test]
fn it_opens_a_modal() {
    let mut app = TestApp::new();
    assert!(!app.palette_open());
    open(&mut app);
    assert!(app.palette_open());
}

#[test]
fn every_command_is_listed() {
    let mut app = TestApp::new();
    open(&mut app);
    assert_eq!(app.palette_row_count(), TestApp::command_names().len());
    let text = app.render_to_text(110, 40).join("\n");
    for label in ["New session", "Agents", "Model", "Provider", "Theme"] {
        assert!(text.contains(label), "`{label}` should be listed: {text}");
    }
}

/// The palette is for looking, not typing: rows read as actions, without
/// the `/` the inline list uses.
#[test]
fn rows_carry_no_slash() {
    let mut app = TestApp::new();
    open(&mut app);
    let text = app.render_to_text(110, 40).join("\n");
    assert!(!text.contains("/new") && !text.contains("/help"), "{text}");
}

/// Browsing, the commands sit under headings; searching, the best match
/// comes first.
#[test]
fn it_groups_when_browsing_and_ranks_when_searching() {
    let mut app = TestApp::new();
    open(&mut app);
    let text = app.render_to_text(110, 40).join("\n");
    // Not "SESSION": the side column's card is called that too.
    assert!(text.contains("AGENTS & MODELS"), "{text}");

    for c in "mo".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    assert_eq!(
        app.palette_selection().as_deref(),
        Some("model"),
        "a label starting with the search ranks first"
    );
    let text = app.render_to_text(110, 40).join("\n");
    assert!(
        !text.contains("AGENTS & MODELS"),
        "no headings in a ranking: {text}"
    );
}

/// The last command is reachable: the list scrolls rather than running off
/// the bottom of the window.
#[test]
fn the_list_scrolls_to_the_last_command() {
    let mut app = TestApp::new();
    open(&mut app);
    for _ in 0..TestApp::command_names().len() {
        app.press_key(KeyCode::Down).expect("down");
    }
    assert_eq!(app.palette_selection().as_deref(), Some("quit"));
    let text = app.render_to_text(100, 24).join("\n");
    assert!(
        text.contains("› Quit"),
        "the selection is on screen: {text}"
    );
}

/// Searching the summary as well as the name is the reason to open a palette
/// rather than type: you remember what it does, not what it is called.
#[test]
fn it_searches_summaries_not_just_names() {
    let mut app = TestApp::new();
    open(&mut app);
    let all = app.palette_row_count();
    for c in "palette".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    assert!(
        app.palette_row_count() < all,
        "typing should narrow the list"
    );

    let mut app = TestApp::new();
    open(&mut app);
    for c in "reasoning".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    assert!(
        app.palette_row_count() >= 1,
        "a word from a summary should still find its command"
    );
}

#[test]
fn arrows_move_the_selection() {
    let mut app = TestApp::new();
    open(&mut app);
    let first = app.palette_selection();
    app.press_key(KeyCode::Down).expect("down");
    let second = app.palette_selection();
    assert_ne!(first, second, "Down should move");
    app.press_key(KeyCode::Up).expect("up");
    assert_eq!(app.palette_selection(), first, "Up should come back");
}

/// The selection must not sit past the end after a search narrows the list.
#[test]
fn narrowing_the_search_resets_the_selection() {
    let mut app = TestApp::new();
    open(&mut app);
    for _ in 0..8 {
        app.press_key(KeyCode::Down).expect("down");
    }
    app.press_key(KeyCode::Char('q')).expect("type");
    let selected = app.palette_selection();
    assert!(selected.is_some(), "something should still be selected");
}

#[test]
fn enter_runs_the_highlighted_command() {
    let mut app = TestApp::new();
    open(&mut app);
    for c in "clear".chars() {
        app.press_key(KeyCode::Char(c)).expect("type");
    }
    app.push_assistant("some content");
    app.press_key(KeyCode::Enter).expect("enter");
    assert!(!app.palette_open(), "running a command closes the palette");
    assert_eq!(app.block_count(), 0, "/clear should have run");
}

/// Picking a command that chooses something opens the window to choose in.
/// Typing its name into the composer would make the palette a slower way of
/// doing what `/` already does.
#[test]
fn choosing_commands_open_their_own_window() {
    for (command, title) in [
        ("agent", " AGENT "),
        ("theme", " THEME "),
        ("provider", " PROVIDER "),
        ("skills", " SKILLS "),
        ("mcp", " MCP SERVERS "),
        // `/resume` is not here: with no saved session in the workspace it
        // correctly reports that rather than opening an empty picker, and a
        // fresh TestApp never has one.
    ] {
        let mut app = TestApp::new();
        open(&mut app);
        assert!(app.select_palette(command), "`{command}` should be listed");
        app.press_key(KeyCode::Enter).expect("enter");
        assert!(!app.palette_open(), "the palette itself should close");
        assert_eq!(
            app.modal_title(),
            title,
            "`/{command}` should have opened its own window"
        );
        assert_eq!(
            app.input_text(),
            "",
            "`/{command}` should not fall back to typing into the composer"
        );
    }
}

/// The window a command opens has to be usable once it is there: the generic
/// picker keys must reach a newly added one.
#[test]
fn the_window_a_command_opens_takes_keys() {
    let mut app = TestApp::new();
    open(&mut app);
    assert!(app.select_palette("agent"));
    app.press_key(KeyCode::Enter).expect("enter");
    assert_eq!(app.modal_title(), " AGENT ");

    let first = app.modal_selection();
    app.press_key(KeyCode::Down).expect("down");
    assert_ne!(app.modal_selection(), first, "Down should move");
    app.press_key(KeyCode::Esc).expect("esc");
    assert!(!app.any_modal_open(), "Esc should close it");
}

/// Picking an agent switches to it, rather than printing a list to read.
#[test]
fn the_agent_window_switches_agent() {
    let mut app = TestApp::new();
    let before = app.active_agent();
    open(&mut app);
    assert!(app.select_palette("agent"));
    app.press_key(KeyCode::Enter).expect("enter");
    app.press_key(KeyCode::Down).expect("down");
    let picked = app.modal_selection().expect("something highlighted");
    app.press_key(KeyCode::Enter).expect("enter");
    assert!(!app.any_modal_open());
    assert_eq!(app.active_agent(), picked, "picking should switch");
    assert_ne!(app.active_agent(), before);
}

/// `/theme` was listed in the palette long before anything handled it, so
/// choosing it answered "Unknown command".
#[test]
fn no_listed_command_is_unhandled() {
    for (name, _) in TestApp::command_names() {
        let mut app = TestApp::new();
        if matches!(name, "quit" | "exit") {
            continue;
        }
        open(&mut app);
        assert!(app.select_palette(name), "`{name}` should be listed");
        app.press_key(KeyCode::Enter).expect("enter");
        let text = app.render_to_text(110, 40).join("\n");
        assert!(
            !text.contains("Unknown command"),
            "`/{name}` is listed but not handled: {text}"
        );
    }
}

#[test]
fn escape_closes_it() {
    let mut app = TestApp::new();
    open(&mut app);
    app.press_key(KeyCode::Esc).expect("esc");
    assert!(!app.palette_open());
}

/// Ctrl+P while something else is open closes that, rather than doing nothing.
#[test]
fn it_closes_another_open_window() {
    let mut app = TestApp::new();
    app.open_settings();
    assert!(app.settings_modal_open(), "settings should be open first");
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert!(!app.settings_modal_open());
    assert!(!app.palette_open(), "one key, one effect");
}

/// The theme picker previews as you move; closing must restore the saved one.
#[test]
fn closing_the_theme_picker_restores_the_saved_theme() {
    let mut app = TestApp::new();
    let before = app.theme_name();
    app.open_themes();
    app.preview_theme(2);
    assert_ne!(app.theme_name(), before);
    app.press(KeyCode::Char('p'), true).expect("ctrl+p");
    assert_eq!(app.theme_name(), before);
}

/// Typing `/` still works — the two routes are independent.
#[test]
fn the_inline_list_still_works() {
    let mut app = TestApp::new();
    app.type_input("/ne");
    let text = app.render_to_text(110, 40).join("\n");
    assert!(text.contains("/new"), "the inline list should still show");
    assert!(!app.palette_open(), "typing does not open the modal");
}

#[test]
fn the_binding_is_documented() {
    let mut app = TestApp::new();
    app.run_command("/help").expect("help");
    let text = app.render_to_text(110, 60).join("\n");
    assert!(text.contains("Ctrl+P"), "help should list it: {text}");
}
