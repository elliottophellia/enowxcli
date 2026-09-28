//! Pasting into whatever is being typed into.
//!
//! An API key is pasted, never typed — nobody types 48 characters of base64
//! by hand. The runtime used to decide where a paste went by listing the form
//! modals by name, so a form missing from that list dropped every paste
//! silently, which is exactly what happened to the TypeSafe key.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn a_paste_reaches_the_composer() {
    let mut app = TestApp::new();
    app.paste("hello from the clipboard");
    assert_eq!(app.input_text(), "hello from the clipboard");
}

/// The case that was broken: a key pasted into the TypeSafe form.
#[test]
fn a_paste_reaches_the_typesafe_key_field() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    app.press(KeyCode::Enter, false).expect("choose API key");
    app.paste("sk-pasted-key-0123456789");
    assert_eq!(
        app.key_draft(),
        "sk-pasted-key-0123456789",
        "a pasted key must land in the field"
    );
}

/// And saving it works the same as a typed one.
#[test]
fn a_pasted_key_can_be_saved() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    app.press(KeyCode::Enter, false).expect("choose API key");
    app.paste("sk-pasted-key-0123456789");
    app.press(KeyCode::Enter, false).expect("save");
    assert_eq!(app.typesafe_key(), "sk-pasted-key-0123456789");
    assert!(app.typesafe_active());
}

/// The provider form is the other place a key gets pasted. It lands in the
/// field being edited, which is the one the cursor is on.
#[test]
fn a_paste_reaches_the_provider_form() {
    let mut app = TestApp::new();
    app.open_custom_provider_form();
    let before = app.settings_field_value();
    app.paste("some-provider-value");
    assert_eq!(
        app.settings_field_value(),
        format!("{before}some-provider-value"),
        "the paste belongs in the field the cursor is on"
    );
}

/// Moving to another field and pasting must fill THAT one.
#[test]
fn a_paste_follows_the_selected_field() {
    let mut app = TestApp::new();
    app.open_custom_provider_form();
    app.press(KeyCode::Tab, false).expect("next field");
    app.press(KeyCode::Tab, false).expect("and again");
    app.paste("sk-the-key");
    assert_eq!(
        app.provider_key_draft(),
        "sk-the-key",
        "two Tabs from the top is the API key row"
    );
}

/// Pasting a terminal's own control sequences must not move the cursor or
/// smuggle newlines into a single-line field.
#[test]
fn control_characters_are_stripped_from_a_pasted_key() {
    let mut app = TestApp::new();
    app.run_command("/typesafe").expect("/typesafe");
    app.press(KeyCode::Enter, false).expect("choose API key");
    app.paste("sk-abc\r\ndef\u{1b}[Dghi\t");
    let draft = app.key_draft();
    assert!(
        !draft.contains('\n') && !draft.contains('\r') && !draft.contains('\u{1b}'),
        "control characters should not survive: {draft:?}"
    );
    assert!(draft.contains("sk-abc"), "the key itself should: {draft:?}");
}

/// A paste lands where the cursor is, not always at the end.
#[test]
fn a_paste_lands_at_the_cursor() {
    let mut app = TestApp::new();
    app.type_input("start end");
    for _ in 0..3 {
        app.press(KeyCode::Left, false).expect("left");
    }
    app.paste("MIDDLE ");
    assert_eq!(app.input_text(), "start MIDDLE end");
}

/// A list has nothing to paste into; a paste there must not land somewhere
/// the user cannot see.
#[test]
fn a_paste_into_a_list_goes_nowhere() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).expect("open palette");
    app.paste("this has nowhere to go");
    assert_eq!(app.input_text(), "", "nothing should reach the composer");
}

/// The MCP form keeps its own fields rather than the settings draft. Routing
/// its paste through the settings draft would write a pasted command into the
/// provider name — the same class of bug as the one this file exists for.
#[test]
fn a_paste_into_the_mcp_form_stays_in_the_mcp_form() {
    let mut app = TestApp::new();
    app.open_mcp_form();
    // The form opens on its first field; step to the command row.
    app.press(KeyCode::Tab, false)
        .expect("to the command field");
    app.paste("npx -y some-server");
    assert_eq!(
        app.mcp_draft_command(),
        "npx -y some-server",
        "the paste belongs to the MCP draft"
    );
    assert_eq!(
        app.provider_key_draft(),
        "",
        "and must not have leaked into the provider settings"
    );
}
