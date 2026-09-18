//! Current tool-row rendering, for judging alignment and hierarchy.
use enowx_tui::testing::TestApp;

fn main() {
    let mut app = TestApp::new();
    app.push_tool(
        "t1",
        "bash",
        r#"{"command":"ls -la && echo \"---\" && cat package.json 2>/dev/null | head -60"}"#,
        "exit 0\na\nb\nc",
    );
    app.push_tool(
        "t2",
        "glob",
        r#"{"pattern":"*.md"}"#,
        "AGENTS.md\nREADME.md\nworkspace/.agents/skills/demo-skill/SKILL.md\nworkspace/AGENTS.md",
    );
    app.push_tool("t3", "read", r#"{"path":"README.md"}"#, &"x\n".repeat(140));
    app.push_tool("t4", "read", r#"{"path":"Cargo.toml"}"#, &"x\n".repeat(48));
    app.push_tool(
        "t5",
        "bash",
        r#"{"command":"find crates web workspace -maxdepth 3 -not -path '*/node_module…"}"#,
        "exit 0\n",
    );
    app.push_tool(
        "t6",
        "grep",
        r#"{"pattern":"fn name"}"#,
        "a.rs:1:fn name\nb.rs:2:fn name",
    );
    for line in app.render_to_text(78, 20) {
        println!("{line}");
    }
}
