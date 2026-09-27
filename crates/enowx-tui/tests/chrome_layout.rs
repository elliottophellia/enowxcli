//! The window's chrome: the chat box's titled edge, the side column's cards,
//! the composer and the status bar.
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

/// The chat box's top edge, which carries its title.
fn chat_edge(app: &mut TestApp) -> String {
    app.main_column(W, H)[0].clone()
}

/// The chat box names where the work is happening, in its own top edge. The
/// agent and its state live in the status bar — saying them in both places
/// left neither able to say anything else.
#[test]
fn the_chat_box_names_the_workspace() {
    let mut app = TestApp::in_conversation();
    let edge = chat_edge(&mut app);
    assert!(
        edge.starts_with("╭─ ws"),
        "the workspace, on the edge: {edge}"
    );
    assert!(
        !edge.contains("orchestrator"),
        "the agent belongs to the status bar, not both: {edge}"
    );
}

/// No header box above the conversation, and no text against the box: one
/// row of padding under the chat box's edge, then the first message, with its
/// marker two columns in from the wall. The header this replaced spent four
/// rows to print one word; text touching the border read as part of it.
#[test]
fn the_conversation_is_padded_from_the_box() {
    let mut app = TestApp::new();
    app.push_user("first message");
    let main = app.main_column(W, H);
    let inside = |row: &str| -> String {
        let chars: Vec<char> = row.chars().collect();
        chars[1..chars.len().saturating_sub(1)].iter().collect()
    };
    assert!(
        inside(&main[1]).trim().is_empty(),
        "a row of padding under the edge: {main:#?}"
    );
    assert!(
        main[2].contains("first message"),
        "then the first message: {main:#?}"
    );
    assert_eq!(
        main[2].chars().position(|c| c == '▌'),
        Some(3),
        "its marker two columns in from the wall: {:?}",
        main[2]
    );
}

/// A paragraph long enough to wrap never runs into the right-hand padding.
#[test]
fn wrapped_text_keeps_clear_of_the_right_wall() {
    let mut app = TestApp::new();
    app.push_assistant(&"a sentence that goes on for a while ".repeat(12));
    for row in app.main_column(W, H) {
        let chars: Vec<char> = row.chars().collect();
        if chars.first() != Some(&'│') || chars.len() < 4 {
            continue;
        }
        let tail: String = chars[chars.len() - 3..chars.len() - 1].iter().collect();
        assert_eq!(
            tail, "  ",
            "two columns of padding before the wall: {row:?}"
        );
    }
}

/// The side column's cards are padded the same way as the chat box.
#[test]
fn the_side_cards_are_padded() {
    let mut app = TestApp::in_conversation();
    let side = app.side_column(W, H);
    assert!(
        side[1]
            .chars()
            .skip(1)
            .collect::<String>()
            .trim_end_matches('│')
            .trim()
            .is_empty(),
        "a row of padding under the SESSION edge: {side:#?}"
    );
    let figures = &side[2];
    assert_eq!(
        figures.chars().position(|c| c.is_alphanumeric()),
        Some(3),
        "the first figure two columns in from the wall: {figures:?}"
    );
}

/// The chat box and the composer share both edges: same left column, same
/// right column. Boxes that nearly line up read worse than none at all.
#[test]
fn the_chat_box_and_composer_line_up() {
    let mut app = TestApp::in_conversation();
    app.type_input("hello");
    let main = app.main_column(W, H);
    let composer_top = main
        .iter()
        .rposition(|row| row.starts_with('╭'))
        .expect("the composer's top edge");
    assert!(composer_top > 0, "a composer below the chat box");
    let right = |row: &str| row.chars().position(|c| c == '╮');
    assert_eq!(
        right(&main[0]),
        right(&main[composer_top]),
        "both boxes should end in the same column:\n{}\n{}",
        main[0],
        main[composer_top]
    );
}

/// A pid and three coloured dots were once in the chrome. Neither is something
/// anyone acts on.
#[test]
fn the_chrome_carries_no_decoration() {
    let mut app = TestApp::new();
    let text = screen(&mut app).join("\n");
    assert!(!text.contains("PID"), "nobody reads a pid: {text}");
    assert!(
        !text.contains("● ● ●"),
        "the terminal already draws a window frame: {text}"
    );
}

/// The side column leads with the SESSION card, and the detail card carries
/// its tabs in its own top edge, where a bordered box names itself.
#[test]
fn the_side_column_titles_its_cards_on_their_edges() {
    let mut app = TestApp::in_conversation();
    let side = app.side_column(W, H);
    assert!(
        side[0].starts_with("╭─ SESSION"),
        "the SESSION card first: {side:#?}"
    );
    assert!(
        side.iter()
            .any(|row| row.starts_with("╭─ Agents · Tools · Skills · Log")),
        "the detail card's tabs on its edge: {side:#?}"
    );
}

/// The selected tab is the one in bold, and it follows the selection — or the
/// strip names as current something that is not showing.
#[test]
fn the_selected_tab_is_marked_on_the_strip() {
    let mut app = TestApp::in_conversation();
    for (tab, name) in [(0, "Agents"), (3, "Log")] {
        app.select_sidebar_tab(tab);
        let rows = screen(&mut app);
        let (y, row) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.contains("Agents · Tools"))
            .expect("the tab strip");
        let bold = app.row_bold(W, H, y as u16);
        let weight = |word: &str| {
            let at = row.find(word).expect("the tab's name");
            bold[row[..at].chars().count()]
        };
        for other in ["Agents", "Tools", "Skills", "Log"] {
            assert_eq!(
                weight(other),
                other == name,
                "only {name} should be bold with tab {tab} selected: {row}"
            );
        }
    }
}

/// The status bar carries state and agent as fixed segments, so each fact is
/// in the same place every time rather than somewhere in a sentence.
#[test]
fn the_status_bar_leads_with_the_state() {
    let mut app = TestApp::in_conversation();
    let idle = app.status_bar(W, H);
    assert!(idle.trim_start().starts_with("READY"), "idle: {idle}");
    assert!(
        idle.contains("orchestrator"),
        "and who is answering: {idle}"
    );

    app.start_fake_turn();
    let busy = app.status_bar(W, H);
    assert!(busy.trim_start().starts_with("WORKING"), "busy: {busy}");
    assert!(!busy.contains("READY"), "and not both at once: {busy}");
}

/// How full the context window is decides whether to compact, so it is always
/// on screen: in the SESSION card beside the chat, or in the status bar when
/// the side column is hidden.
#[test]
fn how_full_the_context_is_is_always_on_screen() {
    let mut app = TestApp::in_conversation();
    app.deliver_usage(12_481, 240, 12_481, 128_000);
    let side = app.side_column(W, H).join("\n");
    assert!(
        side.contains("context") && side.contains('%'),
        "in the card: {side}"
    );
    let narrow = app.status_bar(80, 20);
    assert!(
        narrow.contains("ctx") && narrow.contains('%'),
        "and in the status bar without it: {narrow}"
    );
}

/// On a narrow window the keys win. `draw_split_line` drops the whole right
/// side rather than truncating it, so figures added there must not take the
/// keys with them.
#[test]
fn the_keys_survive_a_narrow_window() {
    let mut app = TestApp::new();
    app.deliver_usage(12_481, 240, 12_481, 128_000);
    for width in [80, 66] {
        let text = app.status_bar(width, 20);
        assert!(
            text.contains("Ctrl+P"),
            "the keys are the point of the row at {width}: {text}"
        );
    }
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
/// is worth a word — in the composer's own edge.
#[test]
fn the_composer_says_when_a_message_will_be_queued() {
    let mut app = TestApp::in_conversation();
    app.start_fake_turn();
    let main = app.main_column(W, H);
    assert!(
        main.iter().any(|row| row.starts_with("╭─ QUEUED")),
        "a turn is running, so Enter does not send now: {main:#?}"
    );
}

/// A user message is marked with a bar rather than boxed. The box spent two
/// rows on rule for every message in the conversation.
#[test]
fn a_user_message_is_marked_not_boxed() {
    let mut app = TestApp::new();
    app.push_user("one line");
    let main = app.main_column(W, H);
    let at = main
        .iter()
        .position(|row| row.contains("one line"))
        .expect("the message");
    assert!(main[at].contains('▌'), "marked with a bar: {}", main[at]);
    // Only the rows around it, and only inside the chat box's walls: the
    // box's own edges are not the message's.
    let inner = |row: &String| -> String {
        let chars: Vec<char> = row.chars().collect();
        chars[1..chars.len().saturating_sub(1)].iter().collect()
    };
    for row in &main[at.saturating_sub(1)..=at + 1] {
        let inside = inner(row);
        assert!(
            !inside.contains("╭─") && !inside.contains("╰─"),
            "the message should not be boxed: {row}"
        );
    }
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

/// The box grows with the message rather than scrolling a one-row field.
#[test]
fn the_box_grows_with_a_multi_line_message() {
    let mut app = TestApp::new();
    app.type_input("one\ntwo\nthree");
    let rows = screen(&mut app);
    let first = rows
        .iter()
        .position(|row| row.contains("❯ one"))
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

/// Which agent is answering and which model it answers with are one fact, so
/// they sit together in the status bar.
#[test]
fn the_model_sits_beside_the_agent() {
    let mut app = TestApp::in_conversation();
    let bar = app.status_bar(W, H);
    let agent = bar.find("orchestrator").expect("the agent") + "orchestrator".len();
    let model = bar.find("no model").expect("the model");
    assert!(model > agent, "the model follows the agent: {bar}");
    // Only the ` · ` separator between them (the dot is two bytes).
    assert!(
        model - agent <= 4,
        "and sits next to it rather than across the bar: {bar}"
    );
    assert!(
        !chat_edge(&mut app).contains("model"),
        "with nothing of it in the chat box's edge"
    );
}

/// They are one block, not one word.
#[test]
fn the_agent_and_model_are_separated() {
    let mut app = TestApp::in_conversation();
    let bar = app.status_bar(W, H);
    assert!(
        bar.contains("orchestrator · no model"),
        "a separator between them: {bar}"
    );
}

/// The pair survives the states the bar changes in, and while working the bar
/// also says what the turn is doing: a clock alone cannot tell a slow model
/// from a long tool call.
#[test]
fn the_model_stays_beside_the_agent_while_working() {
    let mut app = TestApp::in_conversation();
    app.start_fake_turn();
    let bar = app.status_bar(W, H);
    assert!(bar.contains("WORKING"), "the state leads: {bar}");
    assert!(
        bar.contains("orchestrator · no model"),
        "and the pair is still together: {bar}"
    );
    assert!(bar.contains(" · "), "with what the turn is doing: {bar}");
}
