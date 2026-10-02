//! The built-in MCP servers show in /mcp (off by default) and `c` opens their
//! config form.

use crossterm::event::KeyCode;
use enowx_tui::testing::TestApp;

#[test]
fn builtin_servers_are_listed_and_configurable() {
    let mut app = TestApp::new();
    app.run_command("/mcp").expect("open mcp");
    let screen = app.render_to_text(130, 44).join("\n");
    assert!(screen.contains("coolify"), "{screen}");
    assert!(screen.contains("dokploy"), "{screen}");
    assert!(screen.contains("vps"), "{screen}");
    // c opens the config form for the row under the cursor (coolify first).
    app.press_key(KeyCode::Char('c')).expect("config");
    let form = app.render_to_text(130, 44).join("\n");
    assert!(
        form.contains("SET UP COOLIFY") || form.contains("Base URL"),
        "{form}"
    );
}
