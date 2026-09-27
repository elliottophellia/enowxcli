//! Questions from the agent, in a panel fixed above the composer: chosen
//! with the arrows and Enter, a digit or a click; the last row is always
//! "Other" for an answer of one's own; `n` writes a note on an option; with
//! several questions ←/→ move between them and the last Enter sends them all.

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use enowx_core::ask::Reply;
use enowx_tui::testing::TestApp;
use serde_json::{json, Value};

const W: u16 = 120;
const H: u16 = 36;

fn audience(multiple: bool) -> Value {
    json!({
        "question": "Halaman ini untuk siapa terutama?",
        "header": "Untuk siapa",
        "multiple": multiple,
        "options": [
            {"label": "Pasien baru (recommended)", "description": "belum pernah datang"},
            {"label": "Pasien lama", "description": "booking ulang"},
            {"label": "Dokter perujuk"}
        ]
    })
}

fn asked(questions: Vec<Value>) -> TestApp {
    let mut app = TestApp::in_conversation();
    app.push_user("buatkan landing page klinik");
    app.deliver_question("q-1", "orchestrator", json!({ "questions": questions }));
    app
}

fn one(multiple: bool) -> TestApp {
    asked(vec![audience(multiple)])
}

fn replies(app: &TestApp) -> Vec<Reply> {
    app.last_answer().expect("an answer was sent").replies
}

fn first(app: &TestApp) -> Reply {
    replies(app).remove(0)
}

fn press(app: &mut TestApp, code: KeyCode) {
    app.press_key(code).unwrap();
}

#[test]
fn the_question_sits_above_the_composer_with_an_other_row() {
    let mut app = one(false);
    let rows = app.render_to_text(W, H);
    let box_at = rows
        .iter()
        .position(|row| row.contains("QUESTION · orchestrator"))
        .unwrap_or_else(|| panic!("the question box: {rows:#?}"));
    let composer_at = rows
        .iter()
        .position(|row| row.contains("Answer the question above"))
        .expect("the composer points at the panel");
    assert!(box_at < composer_at);
    let screen = rows.join("\n");
    assert!(
        screen.contains("? Halaman ini untuk siapa terutama?"),
        "{screen}"
    );
    assert!(
        screen.contains("› 1  Pasien baru (recommended)"),
        "{screen}"
    );
    assert!(
        screen.contains("4  Other"),
        "the last row is always Other: {screen}"
    );
    let status = rows.last().unwrap();
    assert!(
        status.contains("QUESTION") && status.contains("waiting for your answer"),
        "{status}"
    );
}

#[test]
fn enter_answers_with_the_highlighted_option() {
    let mut app = one(false);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(first(&app).chosen, ["Pasien lama"]);
    assert!(!app.question_open());
}

#[test]
fn a_digit_answers_at_once() {
    let mut app = one(false);
    press(&mut app, KeyCode::Char('3'));
    assert_eq!(first(&app).chosen, ["Dokter perujuk"]);
}

/// The "Other" row takes the user's own words, digits and `n` included.
#[test]
fn the_other_row_takes_an_answer_of_your_own() {
    let mut app = one(false);
    press(&mut app, KeyCode::Char('4'));
    assert!(app.question_open(), "choosing Other starts typing");
    app.type_keys("semua, dalam 2 bahasa");
    press(&mut app, KeyCode::Enter);
    let reply = first(&app);
    assert!(reply.chosen.is_empty());
    assert_eq!(reply.other, "semua, dalam 2 bahasa");
    assert_eq!(app.input_text(), "", "nothing leaked into the composer");
}

#[test]
fn an_empty_other_is_not_an_answer() {
    let mut app = one(false);
    press(&mut app, KeyCode::Char('4'));
    press(&mut app, KeyCode::Enter);
    assert!(app.question_open(), "still waiting for words or a choice");
}

/// `n` writes a note on the highlighted option; Enter keeps it, and it goes
/// with the answer.
#[test]
fn a_note_goes_with_the_answer() {
    let mut app = one(false);
    press(&mut app, KeyCode::Char('n'));
    app.type_keys("banyak lansia, huruf besar");
    press(&mut app, KeyCode::Enter);
    assert!(app.question_open(), "saving the note does not answer");
    let rows = app.render_to_text(W, H).join("\n");
    assert!(rows.contains("note: banyak lansia, huruf besar"), "{rows}");
    press(&mut app, KeyCode::Enter);
    let reply = first(&app);
    assert_eq!(reply.chosen, ["Pasien baru (recommended)"]);
    assert_eq!(
        reply.notes,
        [(
            "Pasien baru (recommended)".to_owned(),
            "banyak lansia, huruf besar".to_owned()
        )]
    );
}

/// Esc while writing a note drops what was typed, not the turn.
#[test]
fn esc_in_a_note_cancels_the_note() {
    let mut app = one(false);
    press(&mut app, KeyCode::Char('n'));
    app.type_keys("tidak jadi");
    press(&mut app, KeyCode::Esc);
    assert!(app.question_open(), "the question still waits");
    press(&mut app, KeyCode::Enter);
    assert!(first(&app).notes.is_empty());
}

#[test]
fn several_answers_are_ticked_with_space() {
    let mut app = one(true);
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Char(' '));
    assert!(app.question_open(), "ticking does not send");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        first(&app).chosen,
        ["Pasien baru (recommended)", "Dokter perujuk"]
    );
}

/// With several questions, Enter answers one and moves to the next, ←/→
/// move freely, and the last Enter sends every reply, an empty one for a
/// question left unanswered.
#[test]
fn several_questions_are_answered_in_turn() {
    let language = json!({"question": "Bahasa?", "header": "Bahasa",
        "options": [{"label": "Indonesia"}, {"label": "Inggris"}]});
    let booking = json!({"question": "Booking lewat apa?", "header": "Booking",
        "options": [{"label": "WhatsApp"}, {"label": "Formulir"}]});
    let mut app = asked(vec![audience(false), language, booking]);
    let rows = app.render_to_text(W, H).join("\n");
    assert!(rows.contains("QUESTION 1 OF 3"), "{rows}");
    assert!(
        rows.contains("1 Untuk siapa   2 Bahasa   3 Booking"),
        "{rows}"
    );

    press(&mut app, KeyCode::Char('2'));
    assert!(app.question_open(), "moved on to the second question");
    let rows = app.render_to_text(W, H).join("\n");
    assert!(
        rows.contains("QUESTION 2 OF 3") && rows.contains("Untuk siapa ✓"),
        "{rows}"
    );

    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Enter);

    let replies = replies(&app);
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0].chosen, ["Pasien lama"]);
    assert!(replies[1].is_empty(), "the second was skipped");
    assert_eq!(replies[2].chosen, ["Formulir"]);
}

#[test]
fn a_click_on_an_option_answers_it() {
    let mut app = one(false);
    let _ = app.render_to_text(W, H);
    let rows = app.question_option_rows();
    assert_eq!(rows.len(), 4, "three options and Other");
    let (x, y) = rows[1];
    app.mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x + 6,
        row: y,
        modifiers: KeyModifiers::NONE,
    })
    .unwrap();
    assert_eq!(first(&app).chosen, ["Pasien lama"]);
}

/// Esc, outside a note, stops the turn as it does whenever one runs.
#[test]
fn esc_stops_the_turn_instead_of_answering() {
    let mut app = one(false);
    press(&mut app, KeyCode::Esc);
    assert!(!app.question_open());
    assert!(app.last_answer().is_none());
}

/// While the panel has the keyboard, letters on an option row type nothing:
/// neither into the composer behind it nor as an answer.
#[test]
fn stray_letters_go_nowhere() {
    let mut app = one(false);
    app.type_keys("xyz");
    assert_eq!(app.input_text(), "");
    assert!(app.question_open());
    let rows = app.render_to_text(W, H);
    assert!(!rows.iter().any(|row| row.contains("COMMANDS")));
}

#[test]
fn a_paste_on_the_other_row_fills_it() {
    let mut app = one(false);
    press(&mut app, KeyCode::Char('4'));
    app.paste("untuk semua\npasien");
    press(&mut app, KeyCode::Enter);
    assert_eq!(first(&app).other, "untuk semua pasien");
}
