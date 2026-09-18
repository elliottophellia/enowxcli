//! The frame's structural lines have to be continuous. A rule that stops
//! partway reads as a rendering fault rather than as the edge of a pane.

use enowx_tui::testing::TestApp;

/// Wide and tall enough for the sidebar to appear.
const W: u16 = 120;
const H: u16 = 20;

fn rows(app: &mut TestApp) -> Vec<String> {
    app.render_to_text(W, H)
}

/// The column the pane divider occupies, found on a row that plainly has one.
fn divider_column(rows: &[String]) -> usize {
    // A body row: starts with the frame, and carries an interior '│'.
    let row = rows
        .iter()
        .find(|r| r.matches('│').count() >= 3)
        .expect("a row with an interior divider");
    let cols: Vec<usize> = row
        .char_indices()
        .filter(|(_, c)| *c == '│')
        .map(|(i, _)| row[..i].chars().count())
        .collect();
    // First and last are the outer frame; the interior one is between them.
    cols[1]
}

#[test]
fn the_pane_divider_runs_the_full_height() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = rows(&mut app);
    let col = divider_column(&rows);

    // Rows between the header rule and the bottom frame must all carry it.
    let top = rows
        .iter()
        .position(|r| r.matches('│').count() >= 3)
        .expect("a body row");
    let bottom = rows
        .iter()
        .rposition(|r| r.contains('╰'))
        .expect("the bottom frame");

    for (i, row) in rows.iter().enumerate().take(bottom).skip(top) {
        let ch = row.chars().nth(col);
        assert_eq!(
            ch,
            Some('│'),
            "row {i} breaks the divider at column {col}: {row:?}"
        );
    }
}

/// The divider must reach the bottom frame, not stop a row above it — that
/// gap is what made it look broken.
#[test]
fn the_divider_meets_the_bottom_frame() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = rows(&mut app);
    let col = divider_column(&rows);
    let bottom = rows
        .iter()
        .rposition(|r| r.contains('╰'))
        .expect("the bottom frame");
    let last_body = &rows[bottom - 1];
    assert_eq!(
        last_body.chars().nth(col),
        Some('│'),
        "the row above the frame should still carry the divider: {last_body:?}"
    );
}

/// Without the sidebar there is no divider to draw, and nothing should be
/// left behind in its column.
#[test]
fn a_narrow_terminal_draws_no_divider() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    // Below the sidebar's width threshold.
    let rows = app.render_to_text(70, H);
    for row in &rows {
        assert!(
            row.matches('│').count() <= 2,
            "a narrow layout should have only the frame: {row:?}"
        );
    }
}
