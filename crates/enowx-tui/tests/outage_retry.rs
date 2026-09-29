//! A turn stopped because the provider stayed down continues by itself a
//! few times, and `/retry` or Esc take over from the wait.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

const OUTAGE: &str =
    "every model tier failed for agent `orchestrator`; tried: enowx/cbc/deepseek-v4.1-flash. \
                      Last error: provider returned 502 Bad Gateway: error code: 502";

#[test]
fn an_outage_is_continued_by_itself_and_esc_cancels_it() {
    let mut app = TestApp::in_conversation();
    app.deliver_error(OUTAGE);
    assert!(app.auto_retry_pending());
    assert!(app.transcript_contains("continues from here in 60s"));
    app.press_key(KeyCode::Esc).unwrap();
    assert!(!app.auto_retry_pending());
}

#[test]
fn only_a_few_times_in_a_row_and_never_for_a_permanent_error() {
    let mut app = TestApp::in_conversation();
    for _ in 0..3 {
        app.deliver_error(OUTAGE);
        assert!(app.auto_retry_pending());
        app.press_key(KeyCode::Esc).unwrap();
    }
    app.deliver_error(OUTAGE);
    assert!(!app.auto_retry_pending());
    assert!(app.transcript_contains("/retry continues from here"));

    // A finished turn starts the count again.
    app.deliver_done("stop");
    app.deliver_error(OUTAGE);
    assert!(app.auto_retry_pending());
    app.press_key(KeyCode::Esc).unwrap();

    app.deliver_error("provider returned 401 Unauthorized: invalid api key");
    assert!(!app.auto_retry_pending());
}

#[test]
fn retry_is_a_command() {
    assert!(TestApp::command_names()
        .iter()
        .any(|(name, _)| *name == "retry"));
}
