//! The window's frame: header, pane labels, composer and status bar.
//!
//! Every row spent on chrome is a row the conversation does not get. These
//! guard the shape that decision produced — and the facts it has to keep
//! showing, which is the part a redesign quietly loses.

use enowx_tui::testing::TestApp;

const W: u16 = 120;
const H: u16 = 24;

fn screen(app: &mut TestApp) -> Vec<String> {
    app.render_to_text(W, H)
}

fn header(app: &mut TestApp) -> String {
    screen(app)[1].clone()
}

fn status_bar(app: &mut TestApp) -> String {
    let rows = screen(app);
    rows[rows.len() - 2].clone()
}

/// The header says what the next keystroke acts on: workspace, then agent.
#[test]
fn the_header_names_the_workspace_and_agent() {
    let mut app = TestApp::new();
    let text = header(&mut app);
    assert!(text.contains("ENX"), "the badge: {text}");
    assert!(text.contains("router"), "the agent answering: {text}");
}

/// A pid and three coloured dots were what used to be here. Neither is
/// something anyone acts on.
#[test]
fn the_header_does_not_carry_decoration() {
    let mut app = TestApp::new();
    let text = header(&mut app);
    assert!(!text.contains("PID"), "nobody reads a pid: {text}");
    assert!(
        !text.contains("● ● ●"),
        "the terminal already draws a window frame: {text}"
    );
}

/// The second row offers keys, and which keys depends on what is on screen —
/// a fixed list would be reference material rather than help.
#[test]
fn the_hint_row_follows_the_state() {
    let mut app = TestApp::new();
    let idle = screen(&mut app)[2].clone();
    assert!(
        idle.contains("Ctrl+P"),
        "the palette is always useful: {idle}"
    );

    app.start_fake_turn();
    let busy = screen(&mut app)[2].clone();
    assert!(
        busy.contains("Ctrl+C"),
        "while working, stopping is the key that matters: {busy}"
    );
}

/// Each pane is named on the rule above it rather than in a row of its own.
#[test]
fn the_panes_are_named_on_the_rule() {
    let mut app = TestApp::new();
    let rule = screen(&mut app)[3].clone();
    assert!(rule.contains("CHAT"), "the conversation side: {rule}");
    // Whichever tab is selected — TOKENS is the one a fresh session opens on.
    assert!(rule.contains("TOKENS"), "and the sidebar's tab: {rule}");
    assert!(rule.contains('─'), "both sitting on a rule: {rule}");
}

/// The sidebar label follows the tab, or it names the wrong pane.
#[test]
fn the_pane_label_follows_the_selected_tab() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(4);
    let rule = screen(&mut app)[3].clone();
    assert!(rule.contains("LOGS"), "the tab that is showing: {rule}");
    assert!(
        !rule.contains("AGENT"),
        "and not the one that is not: {rule}"
    );
}

/// The status bar carries state and agent as fixed segments, so each fact is
/// in the same place every time rather than somewhere in a sentence.
#[test]
fn the_status_bar_leads_with_the_state() {
    let mut app = TestApp::new();
    let idle = status_bar(&mut app);
    assert!(idle.contains("READY"), "idle: {idle}");
    assert!(idle.contains("router"), "and who is answering: {idle}");

    app.start_fake_turn();
    let busy = status_bar(&mut app);
    assert!(busy.contains("WORKING"), "busy: {busy}");
    assert!(!busy.contains("READY"), "and not both at once: {busy}");
}

/// Token spend belongs in the status bar: it is the number that decides
/// whether to compact, and it changes on every turn.
#[test]
fn the_status_bar_shows_what_the_session_has_used() {
    let mut app = TestApp::new();
    app.deliver_usage(12_481, 240, 12_481, 128_000);
    let text = status_bar(&mut app);
    assert!(text.contains("12,481"), "tokens in: {text}");
    assert!(text.contains("240"), "and out: {text}");
    assert!(text.contains("ctx"), "and how full the window is: {text}");
}

/// The composer says what it will do with what is typed. A `/` is a command,
/// not a message, and the difference decides what Enter does.
#[test]
fn the_composer_says_what_it_will_send() {
    let mut app = TestApp::new();
    let rows = screen(&mut app);
    let label = rows
        .iter()
        .find(|row| row.contains("MESSAGE"))
        .expect("a composer label");
    assert!(label.contains('─'), "on the rule: {label}");

    app.type_input("/help");
    let rows = screen(&mut app);
    assert!(
        rows.iter().any(|row| row.contains("COMMAND")),
        "typing a slash should say so: {rows:?}"
    );
}

/// A user message is marked with a bar rather than boxed. The box spent two
/// rows on rule for every message in the conversation.
#[test]
fn a_user_message_is_marked_not_boxed() {
    let mut app = TestApp::new();
    app.push_user("one line");
    let rows = screen(&mut app);
    let message = rows
        .iter()
        .find(|row| row.contains("one line"))
        .expect("the message");
    assert!(message.contains('▌'), "marked with a bar: {message}");
    // The window's own frame draws those corners on its first and last rows;
    // what must be gone is a box inside the chat pane, around the message.
    let boxed = rows[4..rows.len() - 1]
        .iter()
        .any(|row| row.contains("╭─") || row.contains("╰─"));
    assert!(!boxed, "and not boxed: {rows:#?}");
}

/// The point of all of it: more of the window is the conversation. A boxed
/// message spent three rows to show one line of text.
#[test]
fn a_short_message_costs_one_row() {
    let mut app = TestApp::new();
    app.push_user("first");
    let one = screen(&mut app)
        .iter()
        .filter(|row| row.contains('▌'))
        .count();
    app.push_user("second");
    let two = screen(&mut app)
        .iter()
        .filter(|row| row.contains('▌'))
        .count();
    assert_eq!(one, 1, "one line of text, one row");
    assert_eq!(two, 2, "and a second message adds exactly one more");
}
