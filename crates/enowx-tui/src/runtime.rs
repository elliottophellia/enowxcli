use crate::{app::App, modal::Modal, session::TranscriptKind, ui::draw};
use anyhow::Result;
use crossterm::{
    cursor::Show,
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event as TerminalEvent, KeyEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use enowx_core::Config;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture
        )?;
        // Ask the terminal for a blinking bar caret; the drop path restores
        // the terminal's default shape so the outer shell prompt is unchanged.
        use std::io::Write as _;
        let _ = write!(io::stdout(), "\x1b[5 q");
        let _ = io::stdout().flush();
        Ok(guard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use std::io::Write as _;
        let _ = write!(io::stdout(), "\x1b[0 q");
        let _ = io::stdout().flush();
        let _ = execute!(
            io::stdout(),
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen,
            Show
        );
        let _ = disable_raw_mode();
    }
}
pub async fn run(config: Config, session: Option<String>) -> Result<()> {
    use std::io::IsTerminal;
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "enowx-cli requires an interactive terminal"
    );
    let mut app = App::new(config);
    // Refresh the models.dev catalog in the background. Fire-and-forget so a
    // slow or offline network never delays the TUI opening; next run picks
    // up whatever this fetch wrote to `~/.enx/models.json`.
    tokio::spawn(async {
        let _ = enowx_core::catalog::Catalog::refresh().await;
    });
    // Every fresh terminal starts a new conversation; `/resume` opens the
    // picker for users who want to continue a saved one. `enx dev` still
    // reuses a session across rebuilds by passing `--session`.
    if let Some(id) = session {
        if let Err(error) = app.resume(&id) {
            app.push(TranscriptKind::Error, format!("{error:#}"));
        }
    }
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.clear()?;
    while !app.should_quit {
        app.drain_events();
        app.drain_picker_events();
        app.drain_typesafe_check();
        app.refresh_viewed_delegation();
        terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(40))? {
            // Everything already queued is handled before the next frame. A
            // wheel or trackpad sends dozens of events a second, and drawing a
            // long transcript after each one fell behind them: the queue kept
            // scrolling after the hand had stopped.
            let mut handled = 0;
            loop {
                match event::read()? {
                    TerminalEvent::Key(key) if key.kind != KeyEventKind::Release => {
                        if let Err(error) = app.key(key) {
                            if app.modal != Modal::None {
                                app.modal_error = format!("{error:#}");
                            } else {
                                app.push(TranscriptKind::Error, format!("{error:#}"));
                            }
                        }
                    }
                    // Every text-field modal, asked as a question rather than
                    // listed: a form added to `is_form` but missed here would
                    // silently refuse to accept a paste, which is what kept the
                    // TypeSafe key from being pasted at all.
                    TerminalEvent::Paste(text) if app.modal.is_form() => {
                        app.paste(&text);
                    }
                    TerminalEvent::Paste(text) if app.modal == Modal::None => {
                        // A drag-and-drop reaches us as one or more file paths.
                        let paths = crate::attachments::dropped_paths(&text);
                        if !paths.is_empty() {
                            app.attach_paths(&paths);
                        } else if text.trim().is_empty() {
                            // macOS Cmd+V on an image sends a bracketed paste with
                            // no text: terminals cannot stream binary through stdin.
                            app.attach_from_clipboard();
                        } else {
                            // Normalize CR/CRLF to LF, drop other control chars,
                            // and expand tabs so a paste from Warp/iTerm cannot
                            // slip an out-of-band cursor movement into the field.
                            app.paste(&text);
                        }
                    }
                    TerminalEvent::Mouse(mouse) => {
                        if let Err(error) = app.mouse(mouse) {
                            if app.modal != Modal::None {
                                app.modal_error = format!("{error:#}");
                            } else {
                                app.push(TranscriptKind::Error, format!("{error:#}"));
                            }
                        }
                    }
                    _ => {}
                }
                handled += 1;
                if handled >= 512 || app.should_quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
    }
    app.interrupt();
    // Drain while joining: a producer awaiting a full UI channel must be able
    // to publish its final events and persist the cancelled turn before exit.
    if let Some(mut task) = app.task.take() {
        loop {
            tokio::select! {
                result = &mut task => { result?; break; }
                _ = tokio::time::sleep(Duration::from_millis(10)) => app.drain_events(),
            }
        }
    }
    Ok(())
}
