//! A run of calls that only look around (reads, searches, fetches) is one
//! row until opened: `explored  read · grep  2 calls`. What needs the
//! reader's eye keeps its own row: a command, a change, a failure, a call
//! still going.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

const W: u16 = 110;
const H: u16 = 50;

fn transcript() -> TestApp {
    let mut app = TestApp::in_conversation();
    app.push_user("perbaiki timeline");
    app.push_tool(
        "r1",
        "read",
        r#"{"path":"src/data/site.ts"}"#,
        "1\tconst a = 1;",
    );
    app.push_reasoning("The timeline lives in site.ts.");
    app.push_tool(
        "g1",
        "grep",
        r#"{"pattern":"timeline"}"#,
        "src/data/site.ts:4: timeline",
    );
    app.push_tool(
        "b1",
        "bash",
        r#"{"command":"npm run build"}"#,
        "exit 0\nbuilt",
    );
    app.push_tool(
        "e1",
        "edit",
        r#"{"path":"src/data/site.ts","old_text":"a = 1","new_text":"a = 2"}"#,
        "Edited src/data/site.ts at line 1",
    );
    app.push_tool("r2", "read", r#"{"path":"a.ts"}"#, "x");
    app.push_tool("r3", "read", r#"{"path":"b.ts"}"#, "y");
    app.push_tool("b2", "bash", r#"{"command":"npm test"}"#, "exit 1\nfailed");
    app
}

fn click(app: &mut TestApp, id: &str) {
    let _ = app.render_to_text(W, H);
    let y = app
        .tool_header_rows()
        .into_iter()
        .find(|(_, row)| row == id)
        .map(|(y, _)| y)
        .unwrap_or_else(|| panic!("no row for {id}"));
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 8,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
}

#[test]
fn reading_calls_fold_into_one_row() {
    let mut app = transcript();
    let text = app.render_to_text(W, H).join("\n");
    assert!(
        text.contains("explored") && text.contains("2 calls") && text.contains("read · grep"),
        "{text}"
    );
    assert!(
        text.contains("2 calls") && text.contains("read ×2"),
        "{text}"
    );
    assert!(!text.contains("The timeline lives"), "{text}");
    assert!(
        !text.contains("Thought"),
        "a thought between them folds too: {text}"
    );
    // A command, the edit and the failed command each keep their own row.
    assert!(text.contains("npm run build"), "{text}");
    assert!(
        text.contains("├─ edit") && text.contains("src/data/site.ts"),
        "{text}"
    );
    assert!(
        text.contains("npm test") && text.contains("exit 1"),
        "{text}"
    );
}

#[test]
fn a_run_opens_and_closes_on_a_click() {
    let mut app = transcript();
    click(&mut app, "group:r1");
    let rows = app.render_to_text(W, H);
    let text = rows.join("\n");
    assert!(
        text.contains("The timeline lives") || text.contains("Thought"),
        "{text}"
    );
    // Its calls hang off the run's row, on a rail of their own.
    let run = rows.iter().position(|r| r.contains("2 calls")).unwrap();
    let under = &rows[run + 1];
    let at = |row: &str| row.rfind("─ ").unwrap();
    assert!(at(under) > at(&rows[run]), "{}\n{under}", rows[run]);
    assert!(under.contains("site.ts"), "{under}");

    click(&mut app, "group:r1");
    let text = app.render_to_text(W, H).join("\n");
    assert!(!text.contains("The timeline lives"), "closed again: {text}");
    assert!(!text.contains("Thought"), "closed again: {text}");
}

#[test]
fn a_call_still_running_keeps_its_own_row() {
    let mut app = TestApp::in_conversation();
    app.push_tool("r1", "read", r#"{"path":"a.ts"}"#, "x");
    app.push_tool("r2", "read", r#"{"path":"b.ts"}"#, "y");
    app.push_running_tool("r3", "read", r#"{"path":"c.ts"}"#);
    let text = app.render_to_text(W, H).join("\n");
    assert!(text.contains("2 calls"), "{text}");
    assert!(text.contains("c.ts"), "the running read shows: {text}");
}

#[test]
fn a_single_call_is_not_a_run() {
    let mut app = TestApp::in_conversation();
    app.push_tool("r1", "read", r#"{"path":"a.ts"}"#, "x");
    app.push_user("lanjut");
    app.push_tool("r2", "read", r#"{"path":"b.ts"}"#, "y");
    let text = app.main_column(W, H).join("\n");
    assert!(!text.contains("calls"), "{text}");
    assert!(text.contains("a.ts") && text.contains("b.ts"), "{text}");
}
