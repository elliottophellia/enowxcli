//! The Agents tab lists at most five delegations at a time. A wave of
//! parallel work sends out six or eight, and listed whole they pushed the
//! roster off the card. The list follows the newest; the wheel over it, or a
//! click on its "earlier" and "more" rows, slides it.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

const W: u16 = 160;
const H: u16 = 60;

fn delegated(count: usize) -> TestApp {
    let mut app = TestApp::in_conversation();
    for n in 1..=count {
        app.deliver_delegation_started("fe", &format!("build page {n}"), &format!("branch-{n}"));
    }
    app
}

/// The side column's rows without the card's borders.
fn side(app: &mut TestApp) -> Vec<String> {
    app.side_column(W, H)
        .into_iter()
        .map(|row| row.trim_matches(|c| c == '│' || c == ' ').to_owned())
        .collect()
}

fn shown_tasks(app: &mut TestApp) -> Vec<String> {
    side(app)
        .into_iter()
        .filter_map(|row| {
            row.find("build page ")
                .map(|at| row[at..].trim().to_owned())
        })
        .collect()
}

fn wheel(app: &mut TestApp, up: bool) {
    let (x, y) = app.delegation_list_at().expect("the list is on screen");
    app.let_the_wheel_settle();
    app.mouse(MouseEvent {
        kind: if up {
            MouseEventKind::ScrollUp
        } else {
            MouseEventKind::ScrollDown
        },
        column: x + 2,
        row: y + 1,
        modifiers: KeyModifiers::NONE,
    })
    .expect("wheel");
}

#[test]
fn five_or_fewer_are_listed_whole() {
    let mut app = delegated(4);
    let rows = side(&mut app);
    assert_eq!(shown_tasks(&mut app).len(), 4);
    assert!(rows.iter().any(|row| row == "DELEGATED"), "{rows:#?}");
    assert!(!rows
        .iter()
        .any(|row| row.contains("earlier") || row.contains("more")));
}

#[test]
fn a_long_list_shows_the_newest_five_and_says_how_many_are_hidden() {
    let mut app = delegated(8);
    let rows = side(&mut app);
    assert_eq!(
        shown_tasks(&mut app),
        [
            "build page 4",
            "build page 5",
            "build page 6",
            "build page 7",
            "build page 8"
        ]
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("DELEGATED · 8, 8 running")),
        "{rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("↑ 3 earlier")),
        "{rows:#?}"
    );
    assert_eq!(app.delegation_rows(), 10, "two click rows for each shown");
}

#[test]
fn the_wheel_slides_the_list_and_a_new_one_does_not_pull_it_back() {
    let mut app = delegated(8);
    side(&mut app);
    wheel(&mut app, true);
    wheel(&mut app, true);
    assert_eq!(shown_tasks(&mut app)[0], "build page 2");
    let rows = side(&mut app);
    assert!(
        rows.iter().any(|row| row.contains("↑ 1 earlier")),
        "{rows:#?}"
    );
    assert!(rows.iter().any(|row| row.contains("↓ 2 more")), "{rows:#?}");

    // Looking at older ones, a new delegation leaves the view where it is.
    app.deliver_delegation_started("be", "build page 9", "branch-9");
    assert_eq!(shown_tasks(&mut app)[0], "build page 2");

    // Back at the bottom, it follows the newest again.
    for _ in 0..5 {
        wheel(&mut app, false);
    }
    app.deliver_delegation_started("be", "build page 10", "branch-10");
    assert_eq!(
        shown_tasks(&mut app).last().map(String::as_str),
        Some("build page 10")
    );
}

#[test]
fn a_click_on_earlier_slides_a_whole_window() {
    let mut app = delegated(12);
    side(&mut app);
    let (x, y, step) = app
        .delegation_slide_rows()
        .into_iter()
        .find(|(_, _, step)| *step < 0)
        .expect("an earlier row");
    assert_eq!(step, -5);
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x + 1,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
    .expect("click");
    assert_eq!(shown_tasks(&mut app)[0], "build page 3");
}
