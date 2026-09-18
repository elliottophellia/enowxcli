//! The tool row layout: a fixed verb column so arguments start at the same
//! place, and a right-aligned metric so counts form a scannable column.

use enowx_tui::testing::TestApp;

const W: u16 = 78;
const H: u16 = 24;

fn rows_of(app: &mut TestApp) -> Vec<String> {
    app.render_to_text(W, H)
        .into_iter()
        .filter(|r| r.contains('✓') || r.contains('✗'))
        .collect()
}

/// Every tool row must put its argument at the same column, whether or not it
/// has a body to expand — the old layout shifted by two characters and left a
/// zigzag edge down the transcript.
#[test]
fn arguments_line_up_across_row_kinds() {
    let mut app = TestApp::new();
    app.push_tool("t1", "read", r#"{"path":"README.md"}"#, "a\nb");
    app.push_tool("t2", "bash", r#"{"command":"ls"}"#, "exit 0\na");
    app.push_tool("t3", "grep", r#"{"pattern":"fn"}"#, "a.rs:1:fn\nb.rs:2:fn");

    // Anchor on the status icon, then step over the verb column. Searching
    // for the verb or the argument text matched substrings elsewhere in the
    // row — "read" inside "README.md", "ls" inside "lines".
    let cols: Vec<usize> = rows_of(&mut app)
        .iter()
        .filter_map(|row| {
            let icon = row.char_indices().find(|(_, c)| *c == '✓')?.0;
            let rest = &row[icon + '✓'.len_utf8()..];
            let verb_start = rest.find(|c: char| !c.is_whitespace())?;
            let after_verb = rest[verb_start..].find(char::is_whitespace)?;
            let tail = &rest[verb_start + after_verb..];
            let arg = tail.find(|c: char| !c.is_whitespace())?;
            // Column counted in chars, from the icon, so it is comparable
            // across rows regardless of what precedes them.
            Some(row[..icon].chars().count() + verb_start + after_verb + arg)
        })
        .collect();
    assert_eq!(cols.len(), 3, "expected three rows, got {cols:?}");
    assert!(
        cols.windows(2).all(|w| w[0] == w[1]),
        "arguments must start at one column, got {cols:?}"
    );
}

/// The tool name is the anchor of the row, so it has to be there — `bash`
/// rows used to show only `$ command` and never said which tool ran.
#[test]
fn every_row_names_its_tool() {
    let mut app = TestApp::new();
    app.push_tool("t1", "bash", r#"{"command":"cargo build"}"#, "exit 0\n");
    app.push_tool("t2", "read", r#"{"path":"a.rs"}"#, "x");
    let rows = rows_of(&mut app);
    assert!(rows.iter().any(|r| r.contains("bash")), "got {rows:?}");
    assert!(rows.iter().any(|r| r.contains("read")), "got {rows:?}");
}

/// Counts are flush right so they can be compared down the page.
#[test]
fn metrics_are_right_aligned() {
    let mut app = TestApp::new();
    app.push_tool("t1", "read", r#"{"path":"a.rs"}"#, &"x\n".repeat(140));
    app.push_tool("t2", "read", r#"{"path":"b.rs"}"#, &"x\n".repeat(7));
    let ends: Vec<usize> = rows_of(&mut app)
        .iter()
        .filter_map(|r| r.rfind("lines").map(|i| i + "lines".len()))
        .collect();
    assert_eq!(ends.len(), 2, "expected two counted rows");
    assert_eq!(ends[0], ends[1], "counts should end at the same column");
}

/// A long MCP name is clipped rather than pushing the argument column out.
#[test]
fn a_long_tool_name_does_not_break_the_column() {
    let mut app = TestApp::new();
    app.push_tool("t1", "read", r#"{"path":"a.rs"}"#, "x");
    app.push_tool(
        "t2",
        "mcp__github__list_issues_with_a_very_long_name",
        "{}",
        "one\ntwo",
    );
    let rows = rows_of(&mut app);
    for row in &rows {
        assert!(
            row.chars().count() <= W as usize,
            "row overflowed the panel: {row:?}"
        );
    }
    assert!(
        rows.iter().any(|r| r.contains('…')),
        "an over-long tool name should be clipped, got {rows:?}"
    );
}

/// Consecutive calls stack directly; a blank line between each turned a run of
/// ten into twenty rows of mostly empty space.
#[test]
fn consecutive_tool_rows_are_not_separated_by_blanks() {
    let mut app = TestApp::new();
    for i in 0..5 {
        app.push_tool(&format!("t{i}"), "read", r#"{"path":"a.rs"}"#, "x");
    }
    let rendered = app.render_to_text(W, H);
    let first = rendered
        .iter()
        .position(|r| r.contains('✓'))
        .expect("a tool row");
    let last = rendered
        .iter()
        .rposition(|r| r.contains('✓'))
        .expect("a tool row");
    assert_eq!(
        last - first,
        4,
        "five stacked rows should occupy five lines, not ten"
    );
}

/// A failing row carries its exit code beside the count, in the failure colour.
#[test]
fn a_failed_row_shows_its_status() {
    let mut app = TestApp::new();
    app.push_tool_with_status(
        "t1",
        "bash",
        r#"{"command":"cargo build"}"#,
        "exit 101\nerror: could not compile",
        true,
    );
    let styled = app.render_to_styled(W, H);
    assert!(
        styled
            .iter()
            .any(|(text, red)| text.contains("exit 101") && *red),
        "the exit code should render in the failure colour"
    );
}
