//! Wide glyphs (CJK, emoji) occupy two terminal columns. Layout that counts
//! characters instead of columns lets them overrun the panel, and scripts
//! without spaces have no whitespace for a word-wrapper to break on.

use enowx_tui::testing::TestApp;

const W: u16 = 40;
const H: u16 = 24;

fn max_row_width(rows: &[String]) -> usize {
    use unicode_width::UnicodeWidthStr;
    rows.iter().map(|r| r.width()).max().unwrap_or(0)
}

#[test]
fn cjk_paragraph_wraps_within_the_panel() {
    let mut app = TestApp::new();
    // No spaces anywhere: a whitespace-only wrapper cannot break this at all.
    app.push_assistant(&"日本語のテキストです".repeat(12));
    let rows = app.render_to_text(W, H);
    assert!(
        max_row_width(&rows) <= W as usize,
        "a CJK paragraph must wrap inside the panel, widest row was {}",
        max_row_width(&rows)
    );
    assert!(
        rows.iter().filter(|r| r.contains('日')).count() > 1,
        "the paragraph must occupy several rows rather than one long one"
    );
}

#[test]
fn emoji_do_not_overflow_the_panel() {
    let mut app = TestApp::new();
    app.push_assistant(&"🚀🌟✨".repeat(20));
    let rows = app.render_to_text(W, H);
    assert!(
        max_row_width(&rows) <= W as usize,
        "emoji rows must stay inside the panel, widest was {}",
        max_row_width(&rows)
    );
}

#[test]
fn wide_text_in_a_code_block_stays_inside_the_gutter() {
    let mut app = TestApp::new();
    app.push_assistant("```rust\nlet s = \"日本語のテキストはとても長いです\";\n```");
    let rows = app.render_to_text(W, H);
    assert!(
        max_row_width(&rows) <= W as usize,
        "a fenced block must not draw past the panel, widest row was {}",
        max_row_width(&rows)
    );
}

#[test]
fn wide_text_in_a_table_cell_stays_inside_the_panel() {
    let mut app = TestApp::new();
    app.push_assistant("| 名前 | 説明 |\n| --- | --- |\n| 日本語 | とても長い説明文です |");
    let rows = app.render_to_text(W, H);
    assert!(
        max_row_width(&rows) <= W as usize,
        "a table with wide cells must fit the panel, widest row was {}",
        max_row_width(&rows)
    );
}

#[test]
fn ascii_paragraphs_still_break_on_spaces() {
    let mut app = TestApp::new();
    app.push_assistant("the quick brown fox jumps over the lazy dog again and again");
    let rows = app.render_to_text(W, H);
    assert!(max_row_width(&rows) <= W as usize);
    // A width-only cut would slice mid-word; wrapping at spaces must not.
    let body: Vec<&String> = rows.iter().filter(|r| r.contains("quick")).collect();
    assert!(
        !body.is_empty(),
        "the sentence should appear in the transcript"
    );
    for row in rows.iter().filter(|r| !r.trim().is_empty()) {
        assert!(
            !row.trim_end().ends_with("quic") && !row.trim_end().ends_with("brow"),
            "words must not be split mid-word when spaces are available: {row:?}"
        );
    }
}

/// Wrapping by char count fits half as much text per row as the panel allows,
/// so a CJK paragraph that should occupy N rows takes ~2N. This asserts the
/// rows are actually FULL, which is what distinguishes a width-aware wrap from
/// one that merely avoids overflowing.
#[test]
fn cjk_rows_are_filled_to_the_panel_width() {
    use unicode_width::UnicodeWidthStr;
    let mut app = TestApp::new();
    app.push_assistant(&"日本語のテキストです".repeat(12));
    let rows = app.render_to_text(W, H);
    let body: Vec<&String> = rows
        .iter()
        .filter(|r| r.contains('日') || r.contains('テ'))
        .collect();
    assert!(body.len() >= 2, "expected a multi-row paragraph");
    // Every row but the last should use most of the available width.
    for row in &body[..body.len() - 1] {
        assert!(
            row.width() >= (W as usize) - 8,
            "a wrapped row should fill the panel, got width {} in {row:?}",
            row.width()
        );
    }
}
