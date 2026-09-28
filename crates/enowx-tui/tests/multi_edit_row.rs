//! A `multi_edit` reads as one edit row for the file, with a diff per edit.

use enowx_tui::testing::TestApp;

#[test]
fn a_multi_edit_shows_each_change() {
    let mut app = TestApp::in_conversation();
    app.push_user("warnanya pakai token");
    app.push_tool(
        "t1",
        "multi_edit",
        r#"{"path":"styles.css","edits":[{"old_text":"color: red;","new_text":"color: var(--danger);"},{"old_text":"color: blue;","new_text":"color: var(--accent);"}]}"#,
        "Updated styles.css: 2 edits, at lines 4, 12",
    );
    let screen = app.render_to_text(120, 40).join("\n");
    assert!(
        screen.contains("edit") && screen.contains("styles.css"),
        "{screen}"
    );
    assert!(screen.contains("+2 -2 · 2 edits"), "{screen}");
    assert!(
        screen.contains("var(--danger)") && screen.contains("var(--accent)"),
        "{screen}"
    );
}
