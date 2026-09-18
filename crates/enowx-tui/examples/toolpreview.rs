//! Render tool blocks (bash success, bash failure, MCP JSON) so the headers
//! and bodies can be inspected outside a session.
//! Run: cargo run -p enowx-tui --example toolpreview
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.set_show_tool_output(true);

    app.push_tool(
        "t1",
        "bash",
        r#"{"command":"cargo test --workspace --all-features"}"#,
        "exit 0\n   Compiling enowx-tui v0.1.0\n    Finished test profile\ntest result: ok. 62 passed",
    );
    app.push_tool_with_status(
        "t2",
        "bash",
        r#"{"command":"cargo build --release"}"#,
        "exit 101\nerror[E0308]: mismatched types\n  --> src/main.rs:4:9\nerror: could not compile",
        true,
    );
    app.push_tool(
        "t3",
        "mcp__github__list_issues",
        r#"{"repo":"enowdev/enowx"}"#,
        r#"{"issues":[{"number":12,"title":"wrap long diff lines","state":"open"}],"total":1}"#,
    );

    for line in app.render_to_text(78, 34) {
        println!("{line}");
    }
}
