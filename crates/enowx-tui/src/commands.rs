pub(crate) const COMMANDS: [(&str, &str); 26] = [
    ("help", "Show every command"),
    ("commands", "Browse and search every command"),
    ("new", "Start a fresh session"),
    ("resume", "Reopen a saved session in this workspace"),
    (
        "agent",
        "Show the roster, switch agent, or give one its own model (m)",
    ),
    ("model", "Discover or enter a model"),
    ("effort", "Choose how hard the model thinks"),
    ("provider", "Edit provider settings"),
    ("attach", "Attach an image from the workspace"),
    ("theme", "Switch UI theme"),
    ("typesafe", "TypeSafe key and context-saving features"),
    ("skills", "Browse and toggle discovered skills"),
    ("mcp", "Browse, toggle, or add MCP servers"),
    ("rag", "Code search: on or off, database, embedding model"),
    (
        "team",
        "Agents working together: messages, shared board, cross-review",
    ),
    ("compact", "Summarise older turns to free context"),
    (
        "handoff",
        "Carry on in a fresh, light session; keep or delete this one",
    ),
    ("sidebar", "Toggle telemetry right sidebar"),
    ("reasoning", "Open or close every thinking row"),
    ("tools", "Expand or collapse tool output"),
    ("preview", "Let agents look at pages in a browser, or not"),
    ("status", "Show runtime summary"),
    ("clear", "Clear the visible transcript"),
    ("stop", "Interrupt the current turn"),
    ("retry", "Continue from where a failed turn stopped"),
    ("quit", "Leave enowxcli"),
];

/// One command as the Ctrl+P palette shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PaletteRow {
    /// What runs: the name typed after `/`.
    pub(crate) name: &'static str,
    /// What the palette calls it.
    pub(crate) label: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) group: &'static str,
}

/// How the palette groups and names the commands. It is for looking rather
/// than typing, so a row reads as an action ("New session") and not as the
/// name typed after `/`. Every command is in exactly one group.
const PALETTE: [(&str, &[(&str, &str)]); 5] = [
    (
        "Session",
        &[
            ("new", "New session"),
            ("resume", "Resume session"),
            ("compact", "Compact context"),
            ("handoff", "Hand off to a new session"),
            ("clear", "Clear transcript"),
            ("stop", "Stop turn"),
            ("retry", "Retry turn"),
            ("status", "Status"),
        ],
    ),
    (
        "Agents & models",
        &[
            ("agent", "Agents"),
            ("team", "Agents working together"),
            ("model", "Model"),
            ("effort", "Thinking effort"),
            ("provider", "Provider"),
            ("typesafe", "TypeSafe"),
        ],
    ),
    (
        "Context",
        &[
            ("attach", "Attach image"),
            ("skills", "Skills"),
            ("mcp", "MCP servers"),
            ("rag", "Code search (RAG)"),
        ],
    ),
    (
        "View",
        &[
            ("theme", "Theme"),
            ("sidebar", "Sidebar"),
            ("reasoning", "Thinking"),
            ("tools", "Tool output"),
            ("preview", "Browser previews"),
        ],
    ),
    (
        "App",
        &[
            ("commands", "All commands"),
            ("help", "Help"),
            ("quit", "Quit"),
        ],
    ),
];

/// Every command in palette order, group by group.
pub(crate) fn palette() -> Vec<PaletteRow> {
    PALETTE
        .iter()
        .flat_map(|(group, entries)| {
            entries.iter().map(move |(name, label)| PaletteRow {
                name,
                label,
                summary: COMMANDS
                    .iter()
                    .find(|(command, _)| command == name)
                    .map_or("", |(_, summary)| summary),
                group,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_lists_every_command_once() {
        let rows = palette();
        for (name, _) in COMMANDS {
            let count = rows.iter().filter(|row| row.name == name).count();
            assert_eq!(count, 1, "`{name}` should be in the palette exactly once");
        }
        assert_eq!(
            rows.len(),
            COMMANDS.len(),
            "and nothing that is not a command"
        );
        assert!(rows.iter().all(|row| !row.summary.is_empty()));
    }
}
