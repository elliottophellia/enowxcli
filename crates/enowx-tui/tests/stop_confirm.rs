//! A right-click on a sub-agent at work asks before stopping it: the click is
//! easy to make by accident, and a stop cannot be taken back.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_tui::testing::TestApp;

const W: u16 = 160;
const H: u16 = 50;

fn right_click_on(app: &mut TestApp, task: &str) {
    let screen = app.render_to_text(W, H);
    let (row, line) = screen
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains(task))
        .expect("the sub-agent is listed");
    let x = line[..line.find(task).unwrap()].chars().count() as u16;
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: x + 1,
        row: row as u16,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
}

#[test]
fn a_right_click_asks_before_stopping_a_sub_agent() {
    let mut app = TestApp::in_conversation();
    app.deliver_delegation_started("fe", "build the pricing page", "branch-1");

    right_click_on(&mut app, "build the pricing page");
    assert_eq!(
        app.modal_title(),
        " STOP SUB-AGENT ",
        "the click asks, it does not stop"
    );
    let screen = app.render_to_text(W, H).join("\n");
    let name = enowx_core::agent_def::display_name("fe");
    assert!(screen.contains(&format!("Stop {name}?")), "{screen}");
    assert!(screen.contains("Keep"), "{screen}");

    // The dialog opens on Keep: an Enter made by habit stops nothing.
    app.press_key(KeyCode::Enter).unwrap();
    assert!(app.modal_closed());
    assert!(
        app.status_line().contains("keeps working"),
        "{}",
        app.status_line()
    );

    // Asked again, S stops it.
    right_click_on(&mut app, "build the pricing page");
    app.press_key(KeyCode::Char('s')).unwrap();
    assert!(app.modal_closed());
    let status = app.status_line();
    assert!(
        status.contains("stopping") || status.contains("already finished"),
        "{status}"
    );

    // Esc cancels too.
    right_click_on(&mut app, "build the pricing page");
    app.press_key(KeyCode::Esc).unwrap();
    assert!(app.modal_closed());
    assert!(
        app.status_line().contains("keeps working"),
        "{}",
        app.status_line()
    );
}
