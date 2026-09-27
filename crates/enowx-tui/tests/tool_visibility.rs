//! Which tool bodies open on their own.
//!
//! The rule: a body opens when it says something the row does not. A row
//! already carries the tool, its argument and a count, so a list that only
//! restates the argument is noise — and a long run of them buries the
//! conversation the user is actually reading.

use enowx_tui::testing::TestApp;

const W: u16 = 78;
const H: u16 = 30;

fn body_visible(tool: &str, args: &str, result: &str, needle: &str) -> bool {
    let mut app = TestApp::new();
    app.push_tool("t1", tool, args, result);
    app.render_to_text(W, H).iter().any(|r| r.contains(needle))
}

#[test]
fn glob_does_not_dump_its_paths() {
    assert!(
        !body_visible(
            "glob",
            r#"{"pattern":"*.md"}"#,
            "AGENTS.md\nREADME.md",
            "AGENTS.md"
        ),
        "the paths only restate the pattern that produced them"
    );
}

#[test]
fn a_multi_file_read_does_not_dump_its_paths() {
    assert!(
        !body_visible(
            "read",
            r#"{"paths":["a.rs","b.rs","c.rs"]}"#,
            "contents",
            "a.rs"
        ),
        "the contents went to the model, not the reader"
    );
}

#[test]
fn grep_does_not_dump_its_hits() {
    assert!(
        !body_visible(
            "grep",
            r#"{"pattern":"fn main"}"#,
            "src/a.rs:1:fn main\nsrc/b.rs:2:fn main",
            "src/a.rs:1"
        ),
        "a repo-wide search can return hundreds of lines"
    );
}

#[test]
fn bash_does_not_dump_its_output() {
    assert!(
        !body_visible(
            "bash",
            r#"{"command":"ls"}"#,
            "exit 0\nalpha\nbeta",
            "alpha"
        ),
        "even `ls` in a large tree spills hundreds of lines"
    );
}

/// The count is the part worth seeing, so it must survive the collapse.
#[test]
fn the_count_is_still_shown() {
    let mut app = TestApp::new();
    app.push_tool("t1", "glob", r#"{"pattern":"*.md"}"#, "a.md\nb.md\nc.md");
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("3 matches")),
        "the row should still say how much was found, got {rows:?}"
    );
}

/// Collapsed is not hidden: the chevron marks a body that a click will open.
#[test]
fn a_collapsed_row_still_offers_to_open() {
    let mut app = TestApp::new();
    app.push_tool("t1", "glob", r#"{"pattern":"*.md"}"#, "a.md\nb.md");
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains('▸')),
        "a collapsed body must advertise that it can be opened, got {rows:?}"
    );
}

/// An edit is the reason the call was made; the diff opens.
#[test]
fn an_edit_shows_its_diff() {
    assert!(
        body_visible(
            "edit",
            r#"{"path":"a.rs","old_text":"let x = 1;","new_text":"let x = 2;"}"#,
            "edited at line 10",
            "let x = 2;"
        ),
        "the change is the whole point of an edit"
    );
}

/// A todo list is a plan the user follows, not a byproduct.
#[test]
fn a_todo_list_shows_its_items() {
    assert!(
        body_visible(
            "todo",
            r#"{"items":[{"state":"done","label":"wire the parser"}]}"#,
            "",
            "wire the parser"
        ),
        "the plan is for the reader"
    );
}

/// A write is a closed row: which file, how many lines. Its content is a
/// click away, as a card (see `transcript_rows.rs`). Open by default, a page
/// of HTML per write pushed the conversation off the screen.
#[test]
fn a_write_stays_closed_until_opened() {
    assert!(
        !body_visible(
            "write",
            r#"{"path":"a.rs","content":"fn main() {}"}"#,
            "wrote a.rs",
            "fn main()"
        ),
        "the content waits for a click"
    );
}

/// An MCP payload is unknown to us, so it is never assumed to be noise.
#[test]
fn an_mcp_payload_is_shown() {
    assert!(
        body_visible(
            "mcp__github__list_issues",
            "{}",
            r#"{"issues":[{"number":12}]}"#,
            "\"number\""
        ),
        "an unknown payload cannot be judged noise"
    );
}

/// A failure opens regardless of the policy — that is when the output is most
/// needed, and hunting for a chevron to read an error is the wrong default.
#[test]
fn a_failed_call_opens_even_when_its_tool_is_collapsed_by_default() {
    let mut app = TestApp::new();
    app.push_tool_with_status(
        "t1",
        "bash",
        r#"{"command":"cargo build"}"#,
        "exit 101\nerror[E0308]: mismatched types",
        true,
    );
    let rows = app.render_to_text(W, H);
    assert!(
        rows.iter().any(|r| r.contains("E0308")),
        "an error must be readable without a click, got {rows:?}"
    );
}

/// Opening a body is a spot check, not a dump. A repo-wide grep would
/// otherwise materialise every hit, which is the cost the collapse exists to
/// avoid — just deferred to the click.
#[test]
fn an_opened_tree_is_capped() {
    let hits: String = (0..300)
        .map(|i| format!("crates/a/src/f{i}.rs:{i}:fn handler\n"))
        .collect();
    let mut app = TestApp::new();
    app.push_tool("t1", "grep", r#"{"pattern":"fn handler"}"#, &hits);
    // grep is collapsed by policy; this is the click that opens it.
    app.expand_tool("t1");
    let rows = app.render_to_text(W, 200);
    let shown = rows.iter().filter(|r| r.contains("src/f")).count();
    assert!(
        shown <= 16,
        "an opened tree should show a sample, not 300 rows; showed {shown}"
    );
    assert!(
        rows.iter().any(|r| r.contains("more")),
        "the fold should say how much was left out, got {rows:?}"
    );
}

/// A short result is shown whole — the cap must not truncate what already fits.
#[test]
fn a_short_tree_is_not_folded() {
    let mut app = TestApp::new();
    app.push_tool("t1", "grep", r#"{"pattern":"x"}"#, "a.rs:1:x\nb.rs:2:x");
    app.expand_tool("t1");
    let rows = app.render_to_text(W, H);
    assert!(rows.iter().any(|r| r.contains("a.rs:1")));
    assert!(rows.iter().any(|r| r.contains("b.rs:2")));
    assert!(
        !rows.iter().any(|r| r.contains("more")),
        "nothing was left out, so nothing should be claimed"
    );
}
