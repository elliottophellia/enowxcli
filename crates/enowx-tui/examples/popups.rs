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
        "custom" => app.open_custom_provider_form(),
        "providers" => {
            app.seed_key("deepseek", "sk-example");
            app.seed_provider(
                "enowx",
                "https://ai.enowx.id/v1",
                "https://ai.enowx.id/v1/models",
                "sk-example",
                "",
            );
            app.open_providers();
        }
        "models" => {
            app.seed_key("deepseek", "sk-example");
            app.seed_provider(
                "enowx",
                "https://ai.enowx.id/v1",
                "https://ai.enowx.id/v1/models",
                "sk-example",
                "cbc/glm-5",
            );
            app.seed_listing(
                "enowx",
                &[
                    ("cbc/glm-5", Some(200_000)),
                    ("cbc/deepseek-v4.1-flash", Some(1_000_000)),
                    ("cbc/kimi-k2", None),
                ],
            );
            app.seed_listing(
                "deepseek",
                &[("deepseek-flash", None), ("deepseek-v4-pro", None)],
            );
            app.open_model_picker();
        }
        other => app.run_command(&format!("/{other}")).unwrap(),
    }
    for line in app.render_to_text(100, 30) {
        println!("{line}");
    }
}
