//! Render the full chrome so the pane divider can be inspected.
//! Run: cargo run -p enowx-tui --example layoutcheck
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.push_assistant("Some content so the transcript is not empty.");
    for line in app.render_to_text(120, 20) {
        println!("{line}");
    }
}
