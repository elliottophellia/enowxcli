//! Keys as terminals send them, mapped to the key the user pressed.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Backspace as every terminal sends it. Many Linux terminals, SSH sessions
/// and multiplexers send Backspace as `^H` (0x08), which crossterm reads as
/// Ctrl+H; a few pass the raw DEL or BS byte through as a character. Unmapped,
/// Backspace did nothing there (or typed an `h`), and only Delete erased.
pub fn normalize(key: KeyEvent) -> KeyEvent {
    let backspace = match key.code {
        KeyCode::Char('h') => {
            key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SHIFT)
        }
        KeyCode::Char('\x08' | '\x7f') => true,
        _ => false,
    };
    if backspace {
        KeyEvent::new_with_kind(KeyCode::Backspace, KeyModifiers::NONE, key.kind)
    } else {
        key
    }
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

    #[test]
    fn other_keys_are_left_alone() {
        for sent in [
            key(KeyCode::Char('h'), KeyModifiers::NONE),
            key(KeyCode::Char('H'), KeyModifiers::SHIFT),
            key(KeyCode::Char('h'), KeyModifiers::CONTROL | KeyModifiers::ALT),
            key(KeyCode::Delete, KeyModifiers::NONE),
        ] {
            assert_eq!(normalize(sent), sent, "{sent:?}");
        }
    }
}
