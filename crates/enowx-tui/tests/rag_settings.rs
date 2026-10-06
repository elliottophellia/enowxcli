//! Settings > RAG: code search on or off, its database, and where its
//! embeddings come from, with the model, width and reranker.

use crossterm::event::KeyCode;
use enowx_core::builtin_mcp::BuiltinConfig;
use enowx_tui::testing::TestApp;

fn rag_page() -> TestApp {
    let mut app = TestApp::new();
    app.run_command("/rag").expect("/rag");
    app
}

fn screen(app: &mut TestApp) -> String {
    app.render_to_text(150, 44).join("\n")
}

/// Down to the field labelled `label`.
fn go_to(app: &mut TestApp, label: &str) {
    for _ in 0..12 {
        let rows = app.render_to_text(150, 44);
        if rows.iter().any(|row| row.contains(&format!("› {label}"))) {
            return;
        }
        app.press_key(KeyCode::Down).unwrap();
    }
    panic!("no field {label}: {}", screen(app));
}

#[test]
fn rag_command_lands_on_configure_row_without_opening_form() {
    let mut app = rag_page();
    let text = screen(&mut app);
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("rag:configure"));
    assert!(text.contains("Configure code search"), "{text}");
    assert!(
        !app.is_modal_open(),
        "the compound form opens on activation"
    );
}

fn open_rag_form(app: &mut TestApp) {
    app.press_key(KeyCode::Enter).expect("configure RAG");
    assert!(app.is_modal_open(), "Configure opens the validated form");
}

#[test]
fn cancelling_rag_form_returns_to_the_configure_row() {
    let mut app = rag_page();
    app.type_keys("rag:configure");
    assert_eq!(app.settings_row_id().as_deref(), Some("rag:configure"));
    assert!(!app.settings_query().is_empty());
    open_rag_form(&mut app);
    app.press_key(KeyCode::Esc).unwrap();
    assert_eq!(app.active_tab(), "Settings");
    assert_eq!(app.settings_row_id().as_deref(), Some("rag:configure"));
    assert!(!app.settings_query().is_empty());
}

#[test]
fn the_model_and_width_are_picked_from_the_providers() {
    let mut app = rag_page();
    open_rag_form(&mut app);
    go_to(&mut app, "Embedding model");
    app.press_key(KeyCode::Right).unwrap();
    assert!(
        screen(&mut app).contains("voyage-3.5"),
        "{}",
        screen(&mut app)
    );
    go_to(&mut app, "Embedding provider");
    app.press_key(KeyCode::Right).unwrap();
    let text = screen(&mut app);
    assert!(text.contains("OpenAI"), "{text}");
    assert!(text.contains("text-embedding-3-small"), "{text}");
    assert!(text.contains("1536"), "{text}");
    assert!(!text.contains("Reranker"), "OpenAI has none: {text}");
}

#[test]
fn a_custom_endpoint_is_typed_and_saved_off_without_a_key() {
    let mut app = rag_page();
    open_rag_form(&mut app);
    go_to(&mut app, "Embedding provider");
    app.press_key(KeyCode::Right).unwrap();
    app.press_key(KeyCode::Right).unwrap();
    assert!(screen(&mut app).contains("Custom (OpenAI-compatible)"));
    go_to(&mut app, "Base URL");
    app.type_keys("http://localhost:11434/v1");
    go_to(&mut app, "Embedding model");
    app.type_keys("nomic-embed-text");
    go_to(&mut app, "Dimension");
    app.type_keys("768");
    app.press_key(KeyCode::Enter).unwrap();
    let rag = BuiltinConfig::load().unwrap().rag.expect("saved");
    assert_eq!(rag.provider, "custom");
    assert_eq!(rag.base_url, "http://localhost:11434/v1");
    assert_eq!(rag.model, "nomic-embed-text");
    assert_eq!(rag.dimension, 768);
    assert!(!app.is_modal_open(), "successful save closes the form");
    assert_eq!(app.settings_row_id().as_deref(), Some("rag:configure"));
    assert_eq!(app.active_tab(), "Settings");
}

#[test]
fn turning_it_on_needs_a_database() {
    let mut app = rag_page();
    open_rag_form(&mut app);
    app.press_key(KeyCode::Right).unwrap(); // RAG: on
    app.press_key(KeyCode::Enter).unwrap();
    assert!(
        screen(&mut app).contains("a database is needed"),
        "{}",
        screen(&mut app)
    );
}

/// Enter in a built-in server's form saves it (it used to do nothing).
#[test]
fn enter_saves_a_builtin_server_form() {
    let mut app = TestApp::new();
    app.run_command("/mcp").unwrap();
    app.press_key(KeyCode::Char('c')).unwrap(); // coolify
    app.type_keys("https://coolify.example.com");
    app.press_key(KeyCode::Down).unwrap();
    app.type_keys("token-for-test");
    app.press_key(KeyCode::Enter).unwrap();
    let coolify = BuiltinConfig::load().unwrap().coolify.expect("saved");
    assert_eq!(coolify.base_url, "https://coolify.example.com");
}
