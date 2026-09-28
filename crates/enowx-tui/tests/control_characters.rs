//! A control character in drawn text is laid out or dropped, never handed
//! to the terminal. A tab in a model's reasoning moved the terminal's cursor
//! to the next tab stop while the interface believed it moved one cell, so
//! the rest of the row, `▸` included, landed on the side panel and stayed.

use enowx_tui::testing::TestApp;

fn no_control(screen: &str) -> bool {
    !screen.chars().any(|c| c.is_control() && c != '\n')
}

#[test]
fn a_tab_in_live_reasoning_stays_in_the_transcript() {
    let mut app = TestApp::in_conversation();
    app.push_user("perbaiki timeline");
    app.push_live_reasoning("Reading the data.\nNow the\ttimeline\tentries are in site.ts. Then");
    let screen = app.render_to_text(120, 30).join("\n");
    assert!(no_control(&screen), "{screen:?}");
    assert!(
        screen.contains("Now the    timeline    entries"),
        "the tab is laid out as spaces: {screen}"
    );
}

#[test]
fn control_characters_anywhere_never_reach_the_terminal() {
    let mut app = TestApp::in_conversation();
    app.push_user("a\tb\u{1b}[2Jc\rd");
    app.push_assistant("col\tcol\tcol\u{7}");
    app.push_tool(
        "t1",
        "bash",
        "{\"command\":\"printf 'x\\ty'\\tcat\"}",
        "exit 0\nname\tlang\tstars\nenx\tRust\t65",
    );
    let screen = app.render_to_text(120, 30).join("\n");
    assert!(no_control(&screen), "{screen:?}");
}
