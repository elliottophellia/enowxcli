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

struct TerminalGuard {
    /// Whether the terminal took the kitty keyboard flags, and so must be
    /// handed them back on the way out.
    #[cfg_attr(windows, allow(dead_code))]
    enhanced: bool,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        #[allow(unused_mut)]
        let mut guard = Self { enhanced: false };
        // Terminals that speak the kitty keyboard protocol (kitty, WezTerm,
        // foot, Ghostty, Alacritty, iTerm2) can then tell Ctrl+Enter and
        // Shift+Enter from Enter, Esc from an Alt chord, and Ctrl+I from Tab.
        // Elsewhere (Terminal.app, GNOME Terminal, tmux) the legacy encoding
        // stays, and every shortcut has a form that survives it. The Windows
        // console reports keys directly and needs none of this.
        #[cfg(not(windows))]
        if crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false) {
            use crossterm::event::{KeyboardEnhancementFlags, PushKeyboardEnhancementFlags};
            guard.enhanced = execute!(
                io::stdout(),
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )
            .is_ok();
        }
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
        #[cfg(not(windows))]
        if self.enhanced {
            let _ = execute!(io::stdout(), crossterm::event::PopKeyboardEnhancementFlags);
        }
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
        "enowxcli requires an interactive terminal"
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
        app.drain_background();
        app.tick_auto_retry();
        app.tick_queue();
        app.tick_handoff();
        app.tick_resources();
        app.tick_mcp_reload();
        app.catch_up_with_catalog();
        app.drain_picker_events();
        app.drain_typesafe_check();
        app.refresh_viewed_delegation();
        terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(Duration::from_millis(40))? {
            // Everything already queued is handled before the next frame. A
            // wheel or trackpad sends dozens of events a second, and drawing a
            // long transcript after each one fell behind them: the queue kept
            // scrolling after the hand had stopped.
            let mut batch = Vec::new();
            loop {
                batch.push(event::read()?);
                if batch.len() >= 512 || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
            for terminal_event in crate::keymap::coalesce_paste(batch) {
                if app.should_quit {
                    break;
                }
                match terminal_event {
                    TerminalEvent::Key(key) if key.kind != KeyEventKind::Release => {
                        if let Err(error) = app.key(crate::keymap::normalize(key)) {
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
                    // A sub-agent's transcript is read only: nothing is typed
                    // into it, pasted text included.
                    TerminalEvent::Paste(_) if app.viewing.is_some() => {}
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
            }
        }
    }
    app.interrupt();
    // No headless Chrome outlives enx.
    enowx_core::preview::shutdown().await;
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
