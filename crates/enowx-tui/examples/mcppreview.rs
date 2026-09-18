//! Show the MCP editor opened on an existing server.
//! Run: cargo run -p enowx-tui --example mcppreview
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.seed_mcp_server(
        "github",
        "npx",
        &["-y", "@modelcontextprotocol/server-github"],
        &[("GITHUB_TOKEN", "ghp_secret")],
        true,
    );
    app.open_mcp_list();
    app.select_mcp_row(0);
    app.accept_mcp_row().unwrap();
    for line in app.render_to_text(92, 22) {
        println!("{line}");
    }
}
