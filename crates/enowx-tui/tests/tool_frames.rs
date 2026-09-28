//! An opened row and what it opened read as one thing: the row is the top
//! edge of a frame, the body sits between its walls, and every kind of body
//! (a diff, a file, command output, a thought) is framed the same way.

use enowx_tui::testing::TestApp;
use unicode_width::UnicodeWidthStr;

const W: u16 = 110;
const H: u16 = 70;

/// The rows of the frame whose top edge contains `title`, checked to be one
/// straight box: every row as wide as the top, walls in the same columns.
fn frame(rows: &[String], title: &str) -> Vec<String> {
    let top_at = rows
        .iter()
        .position(|r| r.contains(title) && r.contains('╭'))
        .unwrap_or_else(|| panic!("no frame titled {title}: {rows:#?}"));
    let top = &rows[top_at];
    let left = top[..top.find('╭').unwrap()].width();
    let right = left + top[top.find('╭').unwrap()..top.find('╮').unwrap()].width();
    let mut body = Vec::new();
    for row in &rows[top_at + 1..] {
        let cells: Vec<char> = row.chars().collect();
        let at = |col: usize| {
            let mut w = 0;
            for c in &cells {
                if w == col {
                    return *c;
                }
                w += UnicodeWidthStr::width(c.to_string().as_str());
            }
            ' '
        };
        match at(left) {
            '╰' => {
                assert_eq!(at(right), '╯', "bottom edge meets the right wall: {row}");
                return body;
            }
            '│' => {
                assert_eq!(at(right), '│', "ragged wall: {row}");
                body.push(row.clone());
            }
            other => panic!("the frame broke off at {other:?}: {row}"),
        }
    }
    panic!("the frame titled {title} never closed");
}

#[test]
fn an_opened_row_is_the_top_edge_of_its_frame() {
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

    let diff = frame(&rows, "✓ edit  src/a.ts");
    assert!(diff.iter().any(|r| r.contains("const a = 2;")), "{diff:#?}");
    let file = frame(&rows, "✓ write  src/site.ts");
    assert!(
        file.iter().any(|r| r.contains("1  export const site")),
        "{file:#?}"
    );
    let output = frame(&rows, "✓ bash  npm run build");
    assert!(
        output.iter().any(|r| r.contains("built in 366ms")),
        "{output:#?}"
    );
    let thought = frame(&rows, "✻ Thought");
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
    assert!(row.contains("✓ bash") && row.contains('▸'), "{row}");
    assert!(!rows.iter().any(|r| r.contains('╭') && r.contains("bash")));
    assert!(!rows.iter().any(|r| r.contains("built in 366ms")));
}
