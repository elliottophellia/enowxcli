//! Show how repeated errors collapse. Run: cargo run -p enowx-tui --example errorpreview
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.push_user("summarize this repo");
    app.push_assistant("Reading the tree now.");
    for _ in 0..7 {
        app.push_error("Request failed: error sending request for url (https://api.example.com/v1/chat/completions): connection refused");
    }
    for line in app.render_to_text(78, 22) {
        println!("{line}");
    }
}
