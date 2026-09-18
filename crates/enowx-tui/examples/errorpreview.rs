//! Show how repeated errors collapse. Run: cargo run -p enowx-tui --example errorpreview
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.push_user("hallo");

    // The exact sequence from a real backoff: one message per attempt.
    let msg = "provider stream ended without a finish reason; no tools executed";
    for attempt in 2..=6 {
        app.push_retry(msg, attempt, 10);
    }
    for line in app.render_to_text(78, 22) {
        println!("{line}");
    }
}
