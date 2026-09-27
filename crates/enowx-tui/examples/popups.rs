//! Render each popup over the new layout, for inspection.
//! Run: cargo run -q -p enowx-tui --example popups -- <which>
use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "palette".into());
    let mut app = TestApp::new();
    app.push_user("contoh pesan");
    app.push_assistant("Jawaban singkat.");
    match which.as_str() {
        "palette" => app.press(KeyCode::Char('p'), true).unwrap(),
        "settings" => app.open_settings(),
        "providers" => app.open_providers(),
        other => app.run_command(&format!("/{other}")).unwrap(),
    }
    for line in app.render_to_text(100, 30) {
        println!("{line}");
    }
}
