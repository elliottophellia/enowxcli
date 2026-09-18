//! Enter on an MCP server opens it for editing.
//!
//! It used to print the server's tool list into the transcript — something you
//! read once and then scroll past, and which left no way to correct a command
//! or an env var short of editing JSON by hand.

use enowx_tui::testing::TestApp;

fn seeded() -> TestApp {
    let mut app = TestApp::new();
    app.seed_mcp_server(
        "github",
        "npx",
        &["-y", "@modelcontextprotocol/server-github"],
        &[("GITHUB_TOKEN", "ghp_secret")],
        true,
    );
    app.open_mcp_list();
    app
}

#[test]
fn enter_opens_the_editor_prefilled() {
    let mut app = seeded();
    app.select_mcp_row(0);
    app.accept_mcp_row().expect("accept");

    assert!(app.in_mcp_form(), "Enter should open the form");
    let (name, command, args, env) = app.mcp_draft();
    assert_eq!(name, "github");
    assert_eq!(command, "npx");
    assert_eq!(
        args, "-y, @modelcontextprotocol/server-github",
        "args should be editable as the comma-separated form shows them"
    );
    assert_eq!(
        env, "GITHUB_TOKEN=ghp_secret",
        "env should round-trip as KEY=VAL lines"
    );
}

/// The old behaviour is gone: Enter must not dump anything into the chat.
#[test]
fn enter_does_not_write_to_the_transcript() {
    let mut app = seeded();
    let before = app.transcript_len();
    app.select_mcp_row(0);
    app.accept_mcp_row().expect("accept");
    assert_eq!(
        app.transcript_len(),
        before,
        "editing a server should not push a notice into the conversation"
    );
}

/// A server discovered from another tool's config can be edited, but saving
/// writes our own file — silently rewriting a file the user keeps for Claude
/// Code or Cursor would be a surprise.
#[test]
fn editing_a_foreign_entry_says_it_will_be_copied() {
    let mut app = TestApp::new();
    app.seed_mcp_server("from-claude", "uvx", &["some-server"], &[], false);
    app.open_mcp_list();
    app.select_mcp_row(0);
    app.accept_mcp_row().expect("accept");

    assert!(app.in_mcp_form());
    let status = app.status_line();
    assert!(
        status.contains("copies") || status.contains("own config"),
        "the user should be told the original is not being rewritten, got {status:?}"
    );
}

/// Editing one of our own entries needs no such warning.
#[test]
fn editing_our_own_entry_is_plain() {
    let mut app = seeded();
    app.select_mcp_row(0);
    app.accept_mcp_row().expect("accept");
    let status = app.status_line();
    assert!(
        !status.contains("copies"),
        "our own entry is edited in place, got {status:?}"
    );
}

/// The "add new" row still opens an empty form.
#[test]
fn the_add_row_still_opens_a_blank_form() {
    let mut app = seeded();
    let last = 1; // one server plus the add row
    app.select_mcp_row(last);
    app.accept_mcp_row().expect("accept");
    assert!(app.in_mcp_form());
    let (name, command, _, _) = app.mcp_draft();
    assert!(
        name.is_empty() && command.is_empty(),
        "adding should start from a blank form, got {name:?}/{command:?}"
    );
}
