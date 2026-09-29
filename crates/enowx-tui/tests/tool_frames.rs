//! An opened row and what it opened read as one thing: the row keeps its
//! place on the rail, the body hangs under it without a box, and every kind
//! of body (a diff, a file, command output, a thought) hangs the same way.

use enowx_tui::testing::TestApp;

const W: u16 = 110;
const H: u16 = 70;

/// The body rows under the opened row that contains `title` and `▾`: every
/// row down to the next row on the same rail, or the end of the stretch.
/// Checked to be drawn without a frame: the row is not a frame's top edge.
fn body(rows: &[String], title: &str) -> Vec<String> {
    let top_at = rows
        .iter()
        .position(|r| r.contains(title) && r.contains('▾'))
        .unwrap_or_else(|| panic!("no opened row {title}: {rows:#?}"));
    assert!(
        !rows[top_at].contains("╭─ ✓") && !rows[top_at].contains(" ─╮"),
        "the row is a frame's edge: {}",
        rows[top_at]
    );
    let top: Vec<char> = rows[top_at].chars().collect();
    let rail = top
        .iter()
        .position(|c| *c == '├' || *c == '└')
        .unwrap_or_else(|| panic!("the row is not on the rail: {}", rows[top_at]));
    let mut out = Vec::new();
    for row in &rows[top_at + 1..] {
        let cells: Vec<char> = row.chars().collect();
        if matches!(cells.get(rail), Some('├' | '└')) {
            break;
        }
        let hung: String = cells.iter().skip(rail + 3).collect();
        let hung = hung.trim_end().trim_end_matches('│').trim_end();
        if hung.trim().is_empty() {
            break;
        }
        out.push(row.clone());
    }
    assert!(!out.is_empty(), "the row {title} opened nothing");
    out
}

#[test]
fn an_opened_row_hangs_its_body_under_it() {
    let mut app = TestApp::in_conversation();
    app.push_user("perbaiki");
    app.push_tool(
        "e1",
        "edit",
        r#"{"path":"src/a.ts","old_text":"const a = 1;","new_text":"const a = 2;"}"#,
        "Edited src/a.ts at line 3",
    );
    app.push_tool(
        "w1",
        "write",
        r#"{"path":"src/site.ts","content":"export const site = {\n  name: \"[Your name]\",\n};\n"}"#,
        "Wrote src/site.ts",
    );
    app.push_tool(
        "b1",
        "bash",
        r#"{"command":"npm run build"}"#,
        "exit 0\nbuilt in 366ms",
    );
    app.push_reasoning("The build passes. Next I preview it.");
    for id in ["e1", "w1", "b1", "thinking-1"] {
        app.expand_tool(id);
    }
    let rows = app.render_to_text(W, H);

    let diff = body(&rows, "src/a.ts");
    assert!(diff.iter().any(|r| r.contains("const a = 2;")), "{diff:#?}");
    let file = body(&rows, "src/site.ts");
    assert!(
        file.iter().any(|r| r.contains("1  export const site")),
        "{file:#?}"
    );
    let output = body(&rows, "npm run build");
    assert!(
        output.iter().any(|r| r.contains("built in 366ms")),
        "{output:#?}"
    );
    let thought = body(&rows, "✻ Thought");
    assert!(
        thought.iter().any(|r| r.contains("Next I preview it.")),
        "{thought:#?}"
    );
}

#[test]
fn a_closed_row_stays_one_plain_row() {
    let mut app = TestApp::in_conversation();
    app.push_tool(
        "b1",
        "bash",
        r#"{"command":"npm run build"}"#,
        "exit 0\nbuilt in 366ms",
    );
    let rows = app.render_to_text(W, H);
    let row = rows.iter().find(|r| r.contains("npm run build")).unwrap();
    assert!(row.contains("─ bash") && row.contains('▸'), "{row}");
    assert!(!rows.iter().any(|r| r.contains("built in 366ms")));
}

fn page(lines: usize, changed: &[usize]) -> String {
    (1..=lines)
        .map(|n| {
            if changed.contains(&n) {
                format!("line {n} changed\n")
            } else {
                format!("line {n}\n")
            }
        })
        .collect()
}

/// A new file says so and shows its first lines; a replaced file shows what
/// changed, as an edit does, not the whole new file again.
#[test]
fn a_write_shows_a_new_file_or_what_it_changed() {
    let mut app = TestApp::in_conversation();
    let args =
        |content: &str| serde_json::json!({"path": "src/site.ts", "content": content}).to_string();
    app.push_tool(
        "w1",
        "write",
        &args(&page(40, &[])),
        "Created src/site.ts (40 lines)",
    );
    app.push_tool(
        "w2",
        "write",
        &args(&page(40, &[7, 31])),
        "Replaced src/site.ts (40 lines; +2 -2)",
    );
    app.set_tool_before("w2", &page(40, &[]));
    let rows = app.render_to_text(W, H);
    let text = rows.join("\n");
    assert!(text.contains("new · 40 lines ▸"), "{text}");
    assert!(text.contains("+2 -2 ▸"), "{text}");

    app.expand_tool("w1");
    app.expand_tool("w2");
    let rows = app.render_to_text(W, H);
    let new_file = body(&rows, "src/site.ts");
    assert!(
        new_file.iter().any(|r| r.contains(" 1  line 1")),
        "{new_file:#?}"
    );
    assert!(
        new_file.iter().any(|r| r.contains("28 more lines")),
        "{new_file:#?}"
    );

    let second = rows
        .iter()
        .rposition(|r| r.contains("write") && r.contains('▾'))
        .unwrap();
    let replaced = body(&rows[second..], "write");
    let body = replaced.join("\n");
    assert!(
        body.contains("line 7 changed") && body.contains("line 31 changed"),
        "{body}"
    );
    assert!(
        !body.contains("line 20\n") && !body.contains(" line 20 "),
        "unchanged middle is left out: {body}"
    );
}

fn click(app: &mut TestApp, id: &str) {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let _ = app.render_to_text(W, H);
    let y = app
        .tool_header_rows()
        .into_iter()
        .find(|(_, row_id)| row_id == id)
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

/// Long output shows a screenful; the fold opens the rest, and closes it.
#[test]
fn long_output_folds_after_a_screenful() {
    let mut app = TestApp::in_conversation();
    let output: String = (1..=30).map(|n| format!("row {n}\n")).collect();
    app.push_tool(
        "b1",
        "bash",
        r#"{"command":"seq 30"}"#,
        &format!("exit 0\n{output}"),
    );
    app.expand_tool("b1");
    let text = app.render_to_text(W, H).join("\n");
    assert!(
        text.contains("row 11") && !text.contains("row 25"),
        "{text}"
    );
    assert!(text.contains("┈ 18 more lines"), "{text}");

    click(&mut app, "full:b1");
    let text = app.render_to_text(W, H).join("\n");
    assert!(text.contains("row 30"), "the fold opened it all: {text}");
    assert!(text.contains("┈ show less"), "{text}");
}

/// The preview's report, as the reader needs it: no line for the model, no
/// temporary paths, and each screenshot a link to the image.
#[test]
fn a_preview_names_its_screenshots_and_opens_them() {
    let mut app = TestApp::in_conversation();
    let dir = "/var/folders/xx/T/enx-preview-abc";
    let report = format!(
        "Preview of http://localhost:4321/\n\n360px, page 8310px tall: 1 problem\n  touch targets under 44px:\n    - a.tiny 29x28px\n  screenshot: {dir}/360.png\n\n1440px, page 6027px tall: nothing found\n  screenshot: {dir}/1440.png\n\nThe measurements are facts about the rendered page; fix what they show."
    );
    app.push_tool(
        "p1",
        "preview",
        r#"{"url":"http://localhost:4321/"}"#,
        &report,
    );
    app.expand_tool("p1");
    let text = app.render_to_text(W, H).join("\n");
    assert!(text.contains("screenshot  360.png"), "{text}");
    assert!(!text.contains("/var/folders"), "{text}");
    assert!(!text.contains("The measurements are facts"), "{text}");
    assert!(
        !text.contains("Preview of"),
        "the row names the page: {text}"
    );
    let links: Vec<String> = app
        .file_link_areas()
        .into_iter()
        .map(|(_, _, _, path)| path)
        .collect();
    assert!(links.contains(&format!("{dir}/360.png")), "{links:?}");
    assert!(links.contains(&format!("{dir}/1440.png")), "{links:?}");
}
