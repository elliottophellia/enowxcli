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
        if rows.iter().any(|r| r.contains(&format!("› {label}"))) {
            return;
        }
        app.press_key(KeyCode::Down).unwrap();
    }
    panic!("no field {label}");
}

#[test]
fn rag_is_a_section_of_settings() {
    let mut app = rag_page();
    let text = screen(&mut app);
    assert!(text.contains("RAG · CODE SEARCH"), "{text}");
    assert!(text.contains("Embedding provider"), "{text}");
    assert!(text.contains("Voyage AI"), "{text}");
    assert!(text.contains("voyage-code-3"), "{text}");
    assert!(text.contains("rerank-2.5"), "{text}");
    // Listed among the sections on the left.
    assert!(text.contains(" RAG "), "{text}");
}

#[test]
fn the_model_and_width_are_picked_from_the_providers() {
    let mut app = rag_page();
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
    assert!(app.is_modal_open(), "the section stays open");
}

#[test]
fn turning_it_on_needs_a_database() {
    let mut app = rag_page();
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
