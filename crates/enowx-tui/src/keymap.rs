//! Keys as terminals send them, mapped to the key the user pressed.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Backspace as every terminal sends it. Many Linux terminals, SSH sessions
/// and multiplexers send Backspace as `^H` (0x08), which crossterm reads as
/// Ctrl+H; a few pass the raw DEL or BS byte through as a character. Unmapped,
/// Backspace did nothing there (or typed an `h`), and only Delete erased.
pub fn normalize(key: KeyEvent) -> KeyEvent {
    let backspace = match key.code {
        KeyCode::Char('h') => {
            key.modifiers.contains(KeyModifiers::CONTROL)
                && !key
                    .modifiers
                    .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT)
        }
        KeyCode::Char('\x08' | '\x7f') => true,
        _ => false,
    };
    if backspace {
        // A raw BS or DEL keeps its Ctrl, which is how Ctrl+Backspace (erase
        // a word) arrives on some terminals; `^H` itself is the plain key.
        let modifiers = match key.code {
            KeyCode::Char('h') => KeyModifiers::NONE,
            _ => key.modifiers & KeyModifiers::CONTROL,
        };
        return KeyEvent::new_with_kind(KeyCode::Backspace, modifiers, key.kind);
    }
    // AltGr. Windows reports it as Ctrl+Alt, so a German `@` (AltGr+Q), a
    // French `{` or a Polish `ł` arrived as a Ctrl chord and was swallowed by
    // the shortcut handlers: those characters could not be typed at all. A
    // Ctrl+Alt chord that produces anything but an ASCII letter or digit is a
    // character, not a shortcut.
    if let KeyCode::Char(c) = key.code {
        if key
            .modifiers
            .contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
            && !c.is_ascii_alphanumeric()
            && !c.is_control()
        {
            return KeyEvent::new_with_kind(
                KeyCode::Char(c),
                key.modifiers - KeyModifiers::CONTROL - KeyModifiers::ALT,
                key.kind,
            );
        }
    }
    key
}

/// A paste that arrived as keystrokes, folded back into one paste.
///
/// The Windows console has no bracketed paste: Windows Terminal and conhost
/// type a paste in as one key event per character, newlines as Enter. Every
/// line of a multi-line paste was sent as its own message. A run of plain
/// keys read in one batch (far faster than anyone types) that has a newline
/// with more text after it is a paste, and becomes one `Event::Paste`.
/// Anything else (a short burst, a trailing Enter) is left as typed, so a
/// fast typist's Enter still sends.
pub fn coalesce_paste(events: Vec<Event>) -> Vec<Event> {
    fn plain(event: &Event) -> Option<char> {
        let Event::Key(key) = event else { return None };
        if key.kind == KeyEventKind::Release
            || key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        match key.code {
            KeyCode::Char(c) => Some(c),
            KeyCode::Enter => Some('\n'),
            KeyCode::Tab => Some('\t'),
            _ => None,
        }
    }
    let mut out = Vec::with_capacity(events.len());
    let mut run: Vec<Event> = Vec::new();
    let mut text = String::new();
    let flush = |run: &mut Vec<Event>, text: &mut String, out: &mut Vec<Event>| {
        let pasted = text.trim_end_matches('\n').contains('\n') && text.chars().count() >= 4;
        if pasted {
            out.push(Event::Paste(std::mem::take(text)));
            run.clear();
        } else {
            out.append(run);
            text.clear();
        }
    };
    for event in events {
        // Windows reports every key twice, pressed and released; a release
        // neither ends a run nor belongs to one.
        if matches!(&event, Event::Key(key) if key.kind == KeyEventKind::Release) {
            continue;
        }
        if let Some(c) = plain(&event) {
            text.push(c);
            run.push(event);
        } else {
            flush(&mut run, &mut text, &mut out);
            out.push(event);
        }
    }
    flush(&mut run, &mut text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn every_form_of_backspace_is_backspace() {
        for sent in [
            key(KeyCode::Backspace, KeyModifiers::NONE),
            key(KeyCode::Char('h'), KeyModifiers::CONTROL),
            key(KeyCode::Char('\x08'), KeyModifiers::NONE),
            key(KeyCode::Char('\x7f'), KeyModifiers::NONE),
        ] {
            assert_eq!(normalize(sent).code, KeyCode::Backspace, "{sent:?}");
        }
    }

    fn typed(text: &str) -> Vec<Event> {
        text.chars()
            .flat_map(|c| {
                let code = if c == '\n' {
                    KeyCode::Enter
                } else {
                    KeyCode::Char(c)
                };
                [
                    Event::Key(KeyEvent::new_with_kind(
                        code,
                        KeyModifiers::NONE,
                        KeyEventKind::Press,
                    )),
                    Event::Key(KeyEvent::new_with_kind(
                        code,
                        KeyModifiers::NONE,
                        KeyEventKind::Release,
                    )),
                ]
            })
            .collect()
    }

    /// The Windows console types a paste in key by key.
    #[test]
    fn a_paste_typed_in_as_keys_is_one_paste() {
        let events = coalesce_paste(typed("first line\nsecond line"));
        assert_eq!(events, vec![Event::Paste("first line\nsecond line".into())]);
    }

    /// A word and Enter in one batch is someone typing fast, and it sends.
    #[test]
    fn typing_and_enter_is_left_as_keys() {
        let events = coalesce_paste(typed("hello\n"));
        assert_eq!(events.len(), 6);
        assert!(events.iter().all(|e| matches!(e, Event::Key(_))));
        assert_eq!(
            events.last(),
            Some(&Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE
            )))
        );
    }

    #[test]
    fn a_shortcut_ends_the_run() {
        let mut events = typed("ab");
        events.push(Event::Key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        )));
        let out = coalesce_paste(events);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn ctrl_backspace_keeps_its_ctrl() {
        let erased = normalize(key(KeyCode::Char('\x7f'), KeyModifiers::CONTROL));
        assert_eq!(erased, key(KeyCode::Backspace, KeyModifiers::CONTROL));
        let plain = normalize(key(KeyCode::Char('h'), KeyModifiers::CONTROL));
        assert_eq!(plain, key(KeyCode::Backspace, KeyModifiers::NONE));
    }

    /// AltGr arrives on Windows as Ctrl+Alt; what it types is a character.
    #[test]
    fn an_altgr_character_is_typed_not_a_shortcut() {
        for c in ['@', '{', '}', '[', ']', '\\', '|', '~', '€', 'ł'] {
            let typed = normalize(key(
                KeyCode::Char(c),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ));
            assert_eq!(typed.code, KeyCode::Char(c));
            assert!(!typed.modifiers.contains(KeyModifiers::CONTROL), "{c}");
        }
        // A real Ctrl+Alt chord on a letter stays a chord.
        let chord = key(
            KeyCode::Char('k'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert_eq!(normalize(chord), chord);
    }

    #[test]
    fn other_keys_are_left_alone() {
        for sent in [
            key(KeyCode::Char('h'), KeyModifiers::NONE),
            key(KeyCode::Char('H'), KeyModifiers::SHIFT),
            key(
                KeyCode::Char('h'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
            key(KeyCode::Delete, KeyModifiers::NONE),
        ] {
            assert_eq!(normalize(sent), sent, "{sent:?}");
        }
    }
}
