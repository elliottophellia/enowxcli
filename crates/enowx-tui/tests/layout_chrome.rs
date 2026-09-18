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

/// The footer belongs to the chat pane. Running it the full width put a
/// lighter band under the sidebar that belonged to neither pane.
#[test]
fn the_footer_band_stops_at_the_divider() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = app.render_to_text(W, H);
    // The footer names the active agent; searched from the bottom because
    // the sidebar's roster lists that name too.
    let agent = app.active_agent();
    let footer = rows
        .iter()
        .rposition(|r| r.contains(&agent))
        .expect("the footer row");

    let bgs = app.row_backgrounds(W, H, footer as u16);
    let col = divider_column(&rows);
    // Right of the divider must not carry the footer's raised background.
    let right: Vec<&String> = bgs.iter().skip(col + 1).take(20).collect();
    let footer_bg = &bgs[col - 5];
    assert!(
        right.iter().all(|bg| *bg != footer_bg),
        "the footer band leaked past the divider: {right:?} vs {footer_bg:?}"
    );
}

/// The sidebar's pager sits on the sidebar's own background, not on a lighter
/// strip of its own — that read as a second footer inside the pane.
#[test]
fn the_sidebar_pager_shares_the_pane_background() {
    let mut app = TestApp::new();
    for i in 0..60 {
        app.push_assistant(&format!("line {i}"));
    }
    let rows = app.render_to_text(W, H);
    let pager = rows
        .iter()
        .position(|r| r.contains("page"))
        .expect("the pager row");
    // A row inside the sidebar's body, past the tab strip at its top.
    let body = pager - 2;

    let col = divider_column(&rows) + 6;
    let pager_bg = app.row_backgrounds(W, H, pager as u16)[col].clone();
    let body_bg = app.row_backgrounds(W, H, body as u16)[col].clone();
    assert_eq!(
        pager_bg, body_bg,
        "the pager should share the sidebar's background, not a lighter strip"
    );
}

/// Every cell of the divider column must share one background. A single cell
/// painted with the footer's lighter colour stood proud of the dark column
/// below it — one cell wide, but the eye lands on it immediately.
#[test]
fn the_divider_column_is_one_colour_all_the_way_down() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = app.render_to_text(W, H);
    let col = divider_column(&rows);

    let top = rows
        .iter()
        .position(|r| r.matches('│').count() >= 3)
        .expect("a body row");
    let bottom = rows
        .iter()
        .rposition(|r| r.contains('╰'))
        .expect("the bottom frame");

    let reference = app.row_backgrounds(W, H, top as u16)[col].clone();
    for row in top..bottom {
        let bg = app.row_backgrounds(W, H, row as u16)[col].clone();
        assert_eq!(
            bg, reference,
            "row {row} breaks the divider column's colour: {bg} vs {reference}"
        );
    }
}
