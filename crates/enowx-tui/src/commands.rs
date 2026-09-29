pub(crate) const COMMANDS: [(&str, &str); 20] = [
    ("help", "Show every command"),
    ("new", "Start a fresh session"),
    ("resume", "Reopen a saved session in this workspace"),
    ("agent", "Show the roster or switch agent"),
    ("model", "Discover or enter a model"),
    ("effort", "Choose how hard the model thinks"),
    ("provider", "Edit provider settings"),
    ("attach", "Attach an image from the workspace"),
    ("theme", "Switch UI theme"),
    ("typesafe", "TypeSafe key and context-saving features"),
    ("skills", "Browse and toggle discovered skills"),
    ("mcp", "Browse, toggle, or add MCP servers"),
    ("compact", "Summarise older turns to free context"),
    ("sidebar", "Toggle telemetry right sidebar"),
    ("reasoning", "Open or close every thinking row"),
    ("tools", "Expand or collapse tool output"),
    ("status", "Show runtime summary"),
    ("clear", "Clear the visible transcript"),
    ("stop", "Interrupt the current turn"),
    ("quit", "Leave enowx-cli"),
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
            ("clear", "Clear transcript"),
            ("stop", "Stop turn"),
            ("status", "Status"),
        ],
    ),
    (
        "Agents & models",
        &[
            ("agent", "Agents"),
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
        ],
    ),
    (
        "View",
        &[
            ("theme", "Theme"),
            ("sidebar", "Sidebar"),
            ("reasoning", "Thinking"),
            ("tools", "Tool output"),
        ],
    ),
    ("App", &[("help", "Help"), ("quit", "Quit")]),
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
