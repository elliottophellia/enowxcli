//! A checklist is drawn as it stands after the call, not from the call.
//!
//! `todo done` names only the items it ticks. Drawn from its arguments, the
//! row listed those items as pending, the opposite of what just happened.

use enowx_tui::testing::TestApp;

#[test]
fn a_done_call_shows_the_whole_list_with_its_ticks() {
    let mut app = TestApp::new();
    app.set_show_tool_output(true);
    app.push_tool(
        "t1",
        "todo",
        r#"{"op":"done","items":["write index.html","write style.css"]}"#,
        "[x] write index.html\n[x] write style.css\n[ ] check both\n1 remaining",
    );
    let text = app.render_to_text(110, 30).join("\n");
    assert!(text.contains("☑ write index.html"), "{text}");
    assert!(text.contains("☑ write style.css"), "{text}");
    assert!(text.contains("☐ check both"), "{text}");
}

#[test]
fn a_running_set_call_shows_its_items() {
    let mut app = TestApp::new();
    app.set_show_tool_output(true);
    app.push_tool("t1", "todo", r#"{"op":"set","items":["one","two"]}"#, "");
    let text = app.render_to_text(110, 30).join("\n");
    assert!(text.contains("☐ one") && text.contains("☐ two"), "{text}");
}
