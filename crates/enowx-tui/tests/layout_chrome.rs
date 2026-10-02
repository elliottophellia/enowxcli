//! The grid's structural lines have to be continuous, and the columns have to
//! end together. A rule that stops partway, or a column that stops a row short
//! of its neighbour, reads as a rendering fault rather than as a layout.

use enowx_tui::testing::TestApp;

/// Wide and tall enough for the side column to appear.
const W: u16 = 120;
const H: u16 = 20;

const BOX_WALL: [char; 3] = ['╭', '│', '╰'];

/// Every row of a column's left wall is part of a box: a corner or a wall,
/// never a gap. The side column stacks two cards, so its wall runs corner,
/// wall, corner, corner, wall, corner — and still never skips a row.
#[test]
fn the_side_column_wall_runs_unbroken() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let side = app.side_column(W, H);
    assert!(!side.is_empty(), "the side column should be on screen");
    for (i, row) in side.iter().enumerate() {
        let first = row.chars().next();
        assert!(
            first.is_some_and(|c| BOX_WALL.contains(&c)),
            "row {i} breaks the side column's wall: {row:?}"
        );
    }
}

/// The chat column's left wall, from the chat box's top corner to the
/// composer's bottom one, is likewise never interrupted.
#[test]
fn the_main_column_wall_runs_unbroken() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    for (i, row) in app.main_column(W, H).iter().enumerate() {
        let first = row.chars().next();
        assert!(
            first.is_some_and(|c| BOX_WALL.contains(&c)),
            "row {i} breaks the chat column's wall: {row:?}"
        );
    }
}

/// Both columns close on the row directly above the status bar. A column
/// ending a row short of the other leaves a step in the grid's bottom edge.
#[test]
fn both_columns_end_on_the_row_above_the_status_bar() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let rows = app.render_to_text(W, H);
    let (side_x, ..) = app.side_area().expect("the side column");
    let last = &rows[rows.len() - 2];
    assert_eq!(
        last.chars().next(),
        Some('╰'),
        "the composer closes here: {last:?}"
    );
    assert_eq!(
        last.chars().nth(side_x as usize),
        Some('╰'),
        "and so does the side column, on the same row: {last:?}"
    );
}

/// The status bar is its own row. No box runs into it, so nothing is written
/// through a corner.
#[test]
fn no_box_runs_into_the_status_bar() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let status = app.status_bar(W, H);
    assert!(
        !status.chars().any(|c| "│╰╯╭╮─".contains(c)),
        "the status bar should carry no box edge: {status:?}"
    );
    assert!(
        status.contains("READY"),
        "and should be the status bar: {status:?}"
    );
}

/// Without room for the side column there is none, and nothing is left in
/// its place: one column of boxes the full width.
#[test]
fn a_narrow_terminal_draws_no_side_column() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    assert!(app.side_column(70, H).is_empty());
    let rows = app.render_to_text(70, H);
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("╮ ╭") || row.contains("│ │")),
        "no second column of boxes should be drawn: {rows:#?}"
    );
}

/// A short terminal keeps the side column but drops the SESSION card, so the
/// detail card keeps enough rows to be worth having. Its figures go to the
/// status bar instead, so they are still on screen.
#[test]
fn a_short_terminal_moves_the_session_figures_to_the_status_bar() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let side = app.side_column(W, 14).join("\n");
    assert!(
        !side.contains("SESSION"),
        "no SESSION card at 14 rows: {side}"
    );
    let status = app.status_bar(W, 14);
    assert!(status.contains("ctx"), "the figures move here: {status:?}");

    let side = app.side_column(W, 24).join("\n");
    assert!(side.contains("SESSION"), "and come back with room: {side}");
    let status = app.status_bar(W, 24);
    assert!(
        !status.contains("ctx"),
        "said once, in the card, when the card is there: {status:?}"
    );
}

/// The pager is set into the detail card's bottom edge on the card's own
/// background: paging costs no row of the card's content, and does not paint
/// a strip of its own that reads as a second footer.
#[test]
fn the_pager_sits_in_the_detail_cards_bottom_edge() {
    let names: Vec<String> = (0..40).map(|i| format!("skill-{i:02}")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut app = TestApp::new_with_skills(&names);
    app.begin_conversation();
    app.select_sidebar_tab(2);
    let side = app.side_column(W, H);
    let (side_x, side_y, ..) = app.side_area().expect("the side column");
    let last = side.last().expect("the side column's last row");
    assert!(last.starts_with('╰'), "the card's bottom edge: {last:?}");
    assert!(
        last.contains("▲ 1-"),
        "carries where it is scrolled: {last:?}"
    );

    // The same background as a wall cell of the card, a few rows up.
    let pager_row = side_y + side.len() as u16 - 1;
    let pager_col = (side_x as usize) + last.find('▲').map(|b| last[..b].chars().count()).unwrap();
    let pager_bg = app.row_backgrounds(W, H, pager_row)[pager_col].clone();
    let wall_bg = app.row_backgrounds(W, H, pager_row - 2)[side_x as usize].clone();
    assert_eq!(
        pager_bg, wall_bg,
        "the pager should share the card's background"
    );
}

/// Every cell of the side column's left wall shares one background. A cell in
/// a different colour stands proud of the column below it — one cell wide,
/// but the eye lands on it immediately.
#[test]
fn the_side_column_wall_is_one_colour_all_the_way_down() {
    let mut app = TestApp::new();
    app.push_assistant("content");
    let side = app.side_column(W, H);
    let (x, y, ..) = app.side_area().expect("the side column");
    let reference = app.row_backgrounds(W, H, y)[x as usize].clone();
    for row in y..y + side.len() as u16 {
        let bg = app.row_backgrounds(W, H, row)[x as usize].clone();
        assert_eq!(bg, reference, "row {row} breaks the wall's colour");
    }
}

/// Input stays inside the composer: its last line sits on the row above the
/// box's lower edge, and that edge sits on the row above the status bar. Text
/// resting directly on the status bar would read as having fallen out of the
/// field.
#[test]
fn the_input_stays_inside_the_composer() {
    for input in ["halo", "satu\ndua", "satu\ndua\ntiga\nempat"] {
        let mut app = TestApp::in_conversation();
        app.type_input(input);
        let rendered = app.main_column(W, H);
        let last = input.lines().next_back().expect("a last line");
        let text = rendered
            .iter()
            .rposition(|row| row.contains(last))
            .unwrap_or_else(|| panic!("{last:?} should be on screen"));
        assert_eq!(
            text + 2,
            rendered.len(),
            "the last input line should sit directly above the composer's edge"
        );
        assert!(
            rendered[text + 1].starts_with('╰'),
            "the box closes below it: {:?}",
            rendered[text + 1]
        );
    }
}

/// The composer's top edge sits directly above the first input line.
#[test]
fn the_composer_keeps_its_rule_above_the_input() {
    let mut app = TestApp::new();
    app.type_input("satu\ndua");
    let rendered = app.render_to_text(W, H);
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
