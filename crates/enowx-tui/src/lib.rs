//! Terminal interface with reference-inspired chrome and a paged telemetry sidebar.
mod ansi;
mod app;
mod attachments;
mod commands;
pub mod keymap;
mod logs;
mod modal;
mod pricing;
mod runtime;
mod session;
mod syntax;
mod text;
pub mod theme;
mod ui;

pub mod testing;

pub use runtime::run;

/// Render markdown to ANSI-escaped strings at a fixed width.
///
/// Exists so code-block highlighting and wrapping can be inspected from an
/// example binary (`cargo run -p enowx-tui --example mdpreview`) and asserted
/// in tests, without standing up a terminal session. Not used by the running
/// interface, which draws through ratatui directly.
pub fn preview_markdown(text: &str, width: usize) -> Vec<String> {
    ui::preview_markdown(text, width, &theme::THEMES[0])
}

/// See `ui::preview_diff`. Exposed for the `diffpreview` example.
pub fn preview_diff(
    path: &str,
    old: &str,
    new: &str,
    start_line: usize,
    width: usize,
) -> Vec<String> {
    ui::preview_diff(path, old, new, start_line, width, &theme::THEMES[0])
}
