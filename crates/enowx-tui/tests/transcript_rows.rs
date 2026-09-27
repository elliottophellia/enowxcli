//! Thinking and file writes as rows in one list of what the agent did.
//!
//! Thinking was printed whole, paragraphs of slanted text between the steps;
//! every file write opened a twelve-line band with a coloured bar and `··`
//! for each indent. Both are rows now: thinking says how long it took, a
//! write says which file and how long it is, and either opens on a click.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;
use unicode_width::UnicodeWidthStr;

const W: u16 = 110;
const H: u16 = 40;

fn click_row_of(app: &mut TestApp, id: &str) {
    let _ = app.render_to_text(W, H);
    let y = app
        .tool_header_rows()
        .into_iter()
        .find(|(_, row_id)| row_id == id)
        .map(|(y, _)| y)
        .unwrap_or_else(|| panic!("no row for {id}"));
    // On the verb, not the path: the path is a link that opens the file.
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 6,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
}

#[test]
fn finished_thinking_is_one_row_until_opened() {
    let mut app = TestApp::new();
    app.push_user("question");
    app.push_reasoning("The first idea. The second, more careful idea.");
    let text = app.main_column(W, H).join("\n");
    assert!(text.contains("✻ Thought for 6s"), "{text}");
    assert!(!text.contains("careful idea"), "closed by default: {text}");

    click_row_of(&mut app, "thinking-1");
    let text = app.main_column(W, H).join("\n");
    assert!(
        text.contains("│ The first idea. The second, more careful idea."),
        "a click opens it, behind a thin bar: {text}"
    );
}

/// While it streams, the row carries its latest sentence, so a long think is
/// visibly alive rather than an unchanging label.
#[test]
fn streaming_thinking_shows_its_latest_sentence() {
    let mut app = TestApp::new();
    app.deliver_reasoning("Look at the layout first. ");
    app.deliver_reasoning("Now check that every class exists in the CSS.");
    let text = app.main_column(W, H).join("\n");
    assert!(
        text.contains("✻ Thinking… Now check that every class exists in the CSS"),
        "{text}"
    );
    // Once the answer starts, it has finished: it says how long it took.
    app.deliver_assistant_text("Done.");
    let text = app.main_column(W, H).join("\n");
    assert!(text.contains("✻ Thought for"), "{text}");
    assert!(!text.contains("Thinking…"), "{text}");
}

/// Ctrl+R opens every thinking row; a click then closes just one.
#[test]
fn the_reasoning_toggle_opens_every_thinking_row() {
    let mut app = TestApp::new();
    app.push_reasoning("FIRST-THOUGHT");
    app.push_reasoning("SECOND-THOUGHT");
    app.set_show_reasoning(true);
    let text = app.main_column(W, H).join("\n");
    assert!(
        text.contains("FIRST-THOUGHT") && text.contains("SECOND-THOUGHT"),
        "{text}"
    );
    click_row_of(&mut app, "thinking-1");
    let text = app.main_column(W, H).join("\n");
    assert!(
        !text.contains("FIRST-THOUGHT") && text.contains("SECOND-THOUGHT"),
        "{text}"
    );
}

fn page(lines: usize) -> String {
    let mut html =
        String::from("<!DOCTYPE html>\n<html lang=\"id\">\n<head>\n\t<meta charset=\"utf-8\">\n");
    for n in 0..lines.saturating_sub(4) {
        html.push_str(&format!("  <p class=\"row-{n}\">placeholder</p>\n"));
    }
    html
}

fn write(app: &mut TestApp, id: &str, path: &str, content: &str) {
    app.push_tool(
        id,
        "write",
        &serde_json::json!({ "path": path, "content": content }).to_string(),
        "wrote it",
    );
}

/// Closed by default, whatever the tool-output toggle says: a page of HTML
/// per write pushed the conversation off the screen.
#[test]
fn a_file_write_is_a_closed_row_by_default() {
    let mut app = TestApp::new();
    app.set_show_tool_output(true);
    write(&mut app, "w1", "index.html", &page(151));
    let text = app.main_column(W, H).join("\n");
    assert!(
        text.contains("index.html") && text.contains("151 lines ▸"),
        "{text}"
    );
    assert!(
        !text.contains("DOCTYPE"),
        "the content stays closed: {text}"
    );
}

/// Opened, it is a card: name and language on the edge, numbered lines,
/// the rest folded, no `··` for indentation.
#[test]
fn an_opened_write_is_a_file_card() {
    let mut app = TestApp::new();
    write(&mut app, "w1", "site/index.html", &page(151));
    click_row_of(&mut app, "w1");
    let rows = app.main_column(W, H);
    let text = rows.join("\n");
    assert!(text.contains("╭─ index.html · html ─"), "{text}");
    assert!(text.contains("  1  <!DOCTYPE html>"), "{text}");
    assert!(
        text.contains("  4      <meta charset"),
        "a tab reads as spaces: {text}"
    );
    assert!(text.contains("139 more lines"), "{text}");
    assert!(
        !text.contains('·') || !text.contains("··"),
        "no indent dots: {text}"
    );
    // The card's walls line up: every row as wide as its top edge.
    let top = rows.iter().find(|r| r.contains("╭─ index.html")).unwrap();
    let left = top.find('╭').unwrap();
    let width = top.trim_end().width();
    for row in rows
        .iter()
        .filter(|r| r.contains("│ ") && r.contains("placeholder"))
    {
        assert_eq!(row.trim_end().width(), width, "ragged card row: {row:?}");
        assert!(row[left..].starts_with('│'), "{row:?}");
    }
}

/// The first click on a row that starts open closes it. It used to do
/// nothing: the click read the tool-output toggle, the renderer another rule.
#[test]
fn the_first_click_on_an_open_row_closes_it() {
    let mut app = TestApp::new();
    app.set_show_tool_output(true);
    app.push_tool(
        "e1",
        "edit",
        r#"{"path":"a.rs","old_text":"let a = 1;","new_text":"let a = 2;"}"#,
        "edited",
    );
    let before = app.main_column(W, H).join("\n");
    assert!(
        before.contains("let a = 2;"),
        "a diff starts open: {before}"
    );
    click_row_of(&mut app, "e1");
    let after = app.main_column(W, H).join("\n");
    assert!(
        !after.contains("let a = 2;"),
        "one click closes it: {after}"
    );
}

/// The card never runs past the column, however narrow.
#[test]
fn the_card_fits_every_width() {
    for width in [40u16, 60, 80, 120] {
        let mut app = TestApp::new();
        write(
            &mut app,
            "w1",
            "styles.css",
            &"body { margin: 0; padding: 0; color: #333; }\n".repeat(20),
        );
        click_row_of(&mut app, "w1");
        for row in app.main_column(width, H) {
            assert!(row.width() <= width as usize, "{row:?} at {width}");
        }
    }
}

/// On a row that opens, only the path is a link. The whole row used to be
/// one, so a click anywhere launched the file in another app and the row
/// could never be opened in place.
#[test]
fn only_the_path_of_an_opening_row_is_a_link() {
    let mut app = TestApp::new();
    write(&mut app, "w1", "index.html", &page(20));
    let rows = app.render_to_text(W, H);
    let (y, x, width, path) = app
        .file_link_areas()
        .into_iter()
        .find(|(_, _, _, path)| path == "index.html")
        .expect("the path is a link");
    let row = &rows[y as usize];
    let linked: String = row.chars().skip(x as usize).take(width as usize).collect();
    assert_eq!(
        linked, path,
        "the link covers the path and nothing else: {row:?}"
    );
}
