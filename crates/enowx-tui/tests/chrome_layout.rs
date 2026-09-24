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

/// The header's text row, inside its box.
fn header(app: &mut TestApp) -> String {
    screen(app)[2].clone()
}

fn status_bar(app: &mut TestApp) -> String {
    let rows = screen(app);
    rows[rows.len() - 2].clone()
}

/// The header names where the work is happening. The agent and its state
/// live in the status bar — saying them in both places left neither line
/// able to say anything else.
#[test]
fn the_header_names_the_workspace() {
    let mut app = TestApp::new();
    let text = header(&mut app);
    assert!(text.contains("ws"), "the workspace: {text}");
    assert!(
        !text.contains("router"),
        "the agent belongs to the status bar, not both: {text}"
    );
}

/// A box, matching the composer at the other end of the window, so the pair
/// frames the conversation between them.
#[test]
fn the_header_is_a_box() {
    let mut app = TestApp::new();
    let rows = screen(&mut app);
    assert!(rows[1].contains("╭─"), "a top edge: {}", rows[1]);
    assert!(
        rows[2].contains("ws"),
        "the workspace inside it: {}",
        rows[2]
    );
    assert!(rows[3].contains("╰─"), "and a bottom edge: {}", rows[3]);
}

/// The two boxes are inset by the same amount, or they nearly line up, which
/// reads worse than not lining up at all.
#[test]
fn the_header_and_composer_boxes_line_up() {
    let mut app = TestApp::new();
    app.type_input("hello");
    let rows = screen(&mut app);
    let header_edge = rows[1].find("╭─").expect("the header's top edge");
    let composer_edge = rows
        .iter()
        .rposition(|row| row.contains("╭─"))
        .and_then(|at| rows[at].find("╭─"))
        .expect("the composer's top edge");
    assert_eq!(
        header_edge, composer_edge,
        "both boxes should start in the same column"
    );
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

/// The sidebar titles itself on its own top edge.
///
/// The names used to sit on the header's lower edge, one at each end. With the
/// sidebar a box of its own that rule spans the full width and belongs to
/// neither pane, so a label on it pointed at a line that was not that pane's —
/// and the transcript, which is the whole left of the window, needs no label
/// to be recognised.
#[test]
fn the_sidebar_names_its_tab_on_its_own_edge() {
    let mut app = TestApp::new();
    let rows = screen(&mut app);
    // The box's top edge is the row carrying a corner to the right of the
    // header's, which spans the window.
    let edge = rows
        .iter()
        .find(|row| row.contains("╭─ TOKENS"))
        .unwrap_or_else(|| panic!("the sidebar should name its tab: {}", rows.join("\n")));
    assert!(
        edge.contains('─'),
        "and carry it on a rule, as a bordered box titles itself: {edge}"
    );
    // The old labels named the chat pane too. Nothing does now.
    let header_rule = &rows[3];
    assert!(
        !header_rule.contains("CHAT"),
        "the header's edge spans both panes, so it names neither: {header_rule}"
    );
}

/// The title follows the tab, or the box names something that is not showing.
#[test]
fn the_sidebar_title_follows_the_selected_tab() {
    let mut app = TestApp::new();
    app.select_sidebar_tab(4);
    let text = screen(&mut app).join("\n");
    assert!(
        text.contains("╭─ LOGS"),
        "the tab that is showing: {text}"
    );
    assert!(
        !text.contains("╭─ AGENT"),
        "and not the one that is not: {text}"
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

/// How full the context window is decides whether to compact, so it belongs
/// where it is seen without opening a tab. The raw token counts do not: the
/// sidebar carries those in full, and the keys matter more than a number
/// nobody acts on directly.
#[test]
fn the_status_bar_shows_how_full_the_context_is() {
    let mut app = TestApp::new();
    app.deliver_usage(12_481, 240, 12_481, 128_000);
    let text = status_bar(&mut app);
    assert!(text.contains("ctx"), "how full the window is: {text}");
    assert!(text.contains('%'), "as a proportion: {text}");
}

/// On a narrow window the keys win. `draw_split_line` drops the whole right
/// side rather than truncating it, so a count added there would take the
/// keys with it.
#[test]
fn the_keys_survive_a_narrow_window() {
    let mut app = TestApp::new();
    app.deliver_usage(12_481, 240, 12_481, 128_000);
    let rows = app.render_to_text(80, 20);
    let text = rows[rows.len() - 2].clone();
    assert!(
        text.contains("Ctrl+P"),
        "the keys are the point of the row: {text}"
    );
}

/// The composer is a prompt marker and a field. A label saying "MESSAGE" on
/// every frame restated what the marker already says, and a label that is
/// always there stops being read.
#[test]
fn the_composer_is_bare_until_it_has_something_to_say() {
    let mut app = TestApp::new();
    let rows = screen(&mut app);
    assert!(
        !rows.iter().any(|row| row.contains("MESSAGE")),
        "nothing to announce: {rows:#?}"
    );
    assert!(
        rows.iter().any(|row| row.contains('❯')),
        "just the prompt: {rows:#?}"
    );
}

/// A message typed while the model is working is queued, not sent, and that
/// is worth a word.
#[test]
fn the_composer_says_when_a_message_will_be_queued() {
    let mut app = TestApp::new();
    app.start_fake_turn();
    let rows = screen(&mut app);
    assert!(
        rows.iter().any(|row| row.contains("QUEUED")),
        "a turn is running, so Enter does not send now: {rows:#?}"
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
    // The composer is boxed on purpose, and the window has its own frame, so
    // look only at the rows the message itself occupies.
    let at = rows
        .iter()
        .position(|row| row.contains("one line"))
        .expect("the message");
    // …and only at the columns the chat pane occupies. The sidebar is a box
    // of its own, so its corners land on these rows too — a whole-row scan
    // finds them and reports the message as boxed when what is boxed is the
    // pane beside it.
    let bar = message.find('▌').expect("the bar");
    let around: Vec<String> = rows[at - 1..=at + 1]
        .iter()
        .map(|row| row.chars().take(bar + 40).collect())
        .collect();
    assert!(
        !around
            .iter()
            .any(|row| row.contains("╭─") || row.contains("╰─")),
        "the message should not be boxed: {around:#?}"
    );
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

/// The composer is a field, and a border says so where a rule only said
/// "something changes here".
#[test]
fn the_composer_is_a_box() {
    let mut app = TestApp::new();
    app.type_input("hello");
    let rows = screen(&mut app);
    let at = rows
        .iter()
        .position(|row| row.contains("hello"))
        .expect("the input");
    assert!(
        rows[at - 1].contains("╭─"),
        "a top edge above it: {}",
        rows[at - 1]
    );
    assert!(
        rows[at + 1].contains("╰─"),
        "and a bottom edge below: {}",
        rows[at + 1]
    );
    assert!(
        rows[at].contains('❯'),
        "with the marker inside: {}",
        rows[at]
    );
}

/// A command is not a message, and the box says which before Enter decides.
#[test]
fn the_box_grows_with_a_multi_line_message() {
    let mut app = TestApp::new();
    app.type_input("one\ntwo\nthree");
    let rows = screen(&mut app);
    let first = rows
        .iter()
        .position(|row| row.contains("one"))
        .expect("the first line");
    assert!(rows[first - 1].contains("╭─"), "top above the first line");
    assert!(rows[first + 1].contains("two"), "the second line inside");
    assert!(rows[first + 2].contains("three"), "and the third");
    assert!(
        rows[first + 3].contains("╰─"),
        "with the bottom below them: {}",
        rows[first + 3]
    );
}

/// Which agent is answering and which model it answers with are one fact.
/// Split across the window — agent bottom-left, model top-right — the eye
/// had to hunt for the other half.
#[test]
fn the_model_sits_beside_the_agent() {
    let mut app = TestApp::new();
    let bar = status_bar(&mut app);
    let agent = bar.find("router").expect("the agent");
    let model = bar.find("no model").expect("the model");
    assert!(model > agent, "the model follows the agent: {bar}");
    assert!(
        model - agent < 16,
        "and sits next to it rather than across the bar: {bar}"
    );
    assert!(
        !header(&mut app).contains("model"),
        "with nothing left in the header: {}",
        header(&mut app)
    );
}

/// They are one block, not one word.
#[test]
fn the_agent_and_model_are_separated() {
    let mut app = TestApp::new();
    let bar = status_bar(&mut app);
    assert!(
        bar.contains("router · no model"),
        "a separator between them: {bar}"
    );
}

/// The pair survives the states the bar changes in.
#[test]
fn the_model_stays_beside_the_agent_while_working() {
    let mut app = TestApp::new();
    app.start_fake_turn();
    let bar = status_bar(&mut app);
    assert!(bar.contains("WORKING"), "the state leads: {bar}");
    assert!(
        bar.contains("router · no model"),
        "and the pair is still together: {bar}"
    );
}
