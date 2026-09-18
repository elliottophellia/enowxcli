//! Show the command palette as Ctrl+P opens it.
//! Run: cargo run -p enowx-tui --example palettepreview
use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.press(KeyCode::Char('p'), true).unwrap();
    for line in app.render_to_text(92, 26) {
        println!("{line}");
    }
}
