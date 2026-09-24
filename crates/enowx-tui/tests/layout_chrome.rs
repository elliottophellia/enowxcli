//! The frame's structural lines have to be continuous. A rule that stops
//! partway reads as a rendering fault rather than as the edge of a pane.

use enowx_tui::testing::TestApp;

/// Wide and tall enough for the sidebar to appear.
const W: u16 = 120;
const H: u16 = 20;

fn rows(app: &mut TestApp) -> Vec<String> {
    app.render_to_text(W, H)
}

/// The column the sidebar's box starts in.
///
/// Found from its titled top edge, which is unambiguous — the header and the
/// composer are boxed too, but neither carries a name on its opening corner.
/// Counted in chars rather than bytes, since the frame is drawn in multi-byte
/// box glyphs and the two diverge by the time they reach the sidebar.
fn box_column(rows: &[String]) -> usize {
    rows.iter()
        .find_map(|row| {
            let at = row.find("╭─ ")?;
            Some(row[..at].chars().count())
        })
        .filter(|at| *at > 4)
        .expect("the sidebar's titled top edge")
}

/// The sidebar's left border runs unbroken from its top edge to its bottom.
///
/// This replaces a test of the pane divider. The sidebar is a box of its own
/// now, so the continuous structural line is the box's own wall — but the
/// concern is unchanged: a rule that stops partway reads as a rendering fault.
#[test]
fn the_sidebar_box_wall_runs_unbroken() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = rows(&mut app);
    let col = box_column(&rows);

    let top = rows
        .iter()
        .position(|r| r.chars().nth(col) == Some('╭'))
        .expect("the box's top corner");
    let bottom = rows
        .iter()
        .rposition(|r| r.chars().nth(col) == Some('╰'))
        .expect("the box's bottom corner");
    assert!(bottom > top + 1, "the box should enclose something");

    for (i, row) in rows.iter().enumerate().take(bottom).skip(top + 1) {
        assert_eq!(
            row.chars().nth(col),
            Some('│'),
            "row {i} breaks the sidebar's wall at column {col}: {row:?}"
        );
    }
}

/// The box closes on the status bar's row rather than floating above it.
///
/// Closed on four sides, it sat one row clear of the footer with a band of
/// panel underneath — the same rule-stopping-in-mid-air that the old divider's
/// carry-through was written to fix.
#[test]
fn the_sidebar_box_closes_on_the_footer_row() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = rows(&mut app);
    let col = box_column(&rows);
    let frame = rows
        .iter()
        .rposition(|r| r.contains('╰'))
        .expect("the bottom frame");
    // The window frame is the last `╰`; the box's own corner is the one above
    // it, on the row the status bar occupies.
    let closed = rows[frame - 1].chars().nth(col);
    assert_eq!(
        closed,
        Some('╰'),
        "the box should close on the footer's row: {:?}",
        rows[frame - 1]
    );
}

/// Without the sidebar there is no box to draw, and nothing should be left
/// behind in its columns.
#[test]
fn a_narrow_terminal_draws_no_sidebar() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    // Below the sidebar's width threshold.
    let rows = app.render_to_text(70, H);
    // Counting `│` no longer distinguishes a divider from the composer's own
    // box, so look for what a divider actually is: the same column carrying
    // one on row after row, which a two-row box cannot produce.
    let mut runs: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for row in &rows {
        for (column, _) in row.char_indices().filter(|(_, c)| *c == '│') {
            *runs.entry(column).or_default() += 1;
        }
    }
    // The window frame's own two columns are expected to run the full height.
    let tall: Vec<(usize, usize)> = runs.into_iter().filter(|(_, count)| *count > 4).collect();
    assert_eq!(
        tall.len(),
        2,
        "only the window frame should run the height: {tall:?}"
    );
}

/// The footer stops where the sidebar's box begins.
///
/// The box closes on this row, so a status bar running the full width writes
/// its keys straight over the box's lower edge — a corner with text through
/// it — and paints the raised band across cells the box owns.
#[test]
fn the_footer_band_stops_at_the_sidebar_box() {
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
    let col = box_column(&rows);
    // Right of the box's wall must not carry the footer's raised background.
    let right: Vec<&String> = bgs.iter().skip(col + 1).take(20).collect();
    let footer_bg = &bgs[col - 5];
    assert!(
        right.iter().all(|bg| *bg != footer_bg),
        "the footer band leaked into the sidebar's box: {right:?} vs {footer_bg:?}"
    );
}

/// The sidebar's pager sits on the sidebar's own background, not on a lighter
/// strip of its own — that read as a second footer inside the pane.
#[test]
fn the_sidebar_pager_shares_the_pane_background() {
    let mut app = TestApp::new();
    // The AGENT tab, whose sixteen-agent roster is longer than the pane.
    // TOKENS was used here and no longer pages at all: it is five figures and
    // a bar now, which is the point of the rework — but a tab that fits on one
    // page has no pager for this to be about.
    app.select_sidebar_tab(3);
    let rows = app.render_to_text(W, H);
    let pager = rows
        .iter()
        .position(|r| r.contains("page"))
        .expect("the pager row");
    // A row inside the sidebar's body, past the tab strip at its top.
    let body = pager - 2;

    let col = box_column(&rows) + 6;
    let pager_bg = app.row_backgrounds(W, H, pager as u16)[col].clone();
    let body_bg = app.row_backgrounds(W, H, body as u16)[col].clone();
    assert_eq!(
        pager_bg, body_bg,
        "the pager should share the sidebar's background, not a lighter strip"
    );
}

/// Every cell of the box's left wall must share one background. A single cell
/// painted with the footer's lighter colour stood proud of the dark column
/// below it — one cell wide, but the eye lands on it immediately.
#[test]
fn the_sidebar_wall_is_one_colour_all_the_way_down() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = app.render_to_text(W, H);
    let col = box_column(&rows);

    let top = rows
        .iter()
        .position(|r| r.chars().nth(col) == Some('╭'))
        .expect("the box's top corner");
    let bottom = rows
        .iter()
        .rposition(|r| r.chars().nth(col) == Some('╰'))
        .expect("the box's bottom corner");

    let reference = app.row_backgrounds(W, H, top as u16)[col].clone();
    for row in top..=bottom {
        let bg = app.row_backgrounds(W, H, row as u16)[col].clone();
        assert_eq!(
            bg, reference,
            "row {row} breaks the sidebar wall's colour: {bg} vs {reference}"
        );
    }
}

/// The last line of input must not sit directly on the footer. Without a gap
/// the text reads as though it has fallen out of the composer onto the black
/// band below it.
#[test]
fn the_composer_keeps_a_gap_above_the_footer() {
    for input in ["halo", "satu\ndua", "satu\ndua\ntiga\nempat"] {
        let mut app = TestApp::new();
        app.type_input(input);
        let rendered = rows(&mut app);
        let footer = rendered
            .iter()
            .rposition(|row| row.contains("router"))
            .expect("the footer names the agent");
        let last = input.lines().next_back().expect("a last line");
        let text = rendered
            .iter()
            .rposition(|row| row.contains(last))
            .unwrap_or_else(|| panic!("{last:?} should be on screen"));
        assert!(
            text < footer,
            "input at {text} should sit above the footer at {footer}"
        );
        // The composer is a box: its lower edge, then a blank row, then the
        // status bar. A border resting directly on the bar reads as part of
        // it rather than as the edge of the field.
        assert_eq!(
            footer - text,
            3,
            "the box's lower edge and one blank row belong between the last \
             input line ({:?}) and the footer ({:?})",
            rendered[text],
            rendered[footer]
        );
        assert!(
            rendered[text + 1].contains("╰─"),
            "the box closes first: {:?}",
            rendered[text + 1]
        );
        // Only the composer's own column: the sidebar beside it has its own
        // content on that row and is not what this is about.
        let gap = &rendered[text + 2];
        let composer = gap.split('│').nth(1).expect("the composer pane's cells");
        assert!(
            composer.trim().is_empty(),
            "the composer's row between them should be blank: {composer:?}"
        );
    }
}

/// The rule above the composer is the other edge of the same gap: input must
/// not touch it either.
#[test]
fn the_composer_keeps_its_rule_above_the_input() {
    let mut app = TestApp::new();
    app.type_input("satu\ndua");
    let rendered = rows(&mut app);
    let first = rendered
        .iter()
        .position(|row| row.contains("❯ satu"))
        .expect("the first input line");
    assert!(
        rendered[first - 1].contains('─'),
        "a rule belongs directly above the input: {:?}",
        rendered[first - 1]
    );
}
