//! Pressing keys on the user's behalf, by writing them to the focused pane.
//!
//! This is how a click reaches a nested session: its keys go to the pane it
//! runs in, exactly as typing them would, rather than to this session's own
//! keybindings.

use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::{BareKey, KeyModifier, KeyWithModifier};

const ESC: u8 = 0x1b;

/// The actions that press `keys` in order in the focused pane.
///
/// Each carries the key itself as well as its bytes. A pane that asked for the
/// kitty keyboard protocol, as Zellij does, gets the key re-encoded that way by
/// the host, so the bytes only matter to a pane using the legacy encoding.
pub fn press_keys(keys: &[KeyWithModifier]) -> Vec<Action> {
    keys.iter()
        .map(|key| Action::Write {
            key_with_modifier: Some(key.clone()),
            bytes: legacy_bytes(key),
            is_kitty_keyboard_protocol: false,
        })
        .collect()
}

/// `key` as an xterm-style terminal sends it, without the kitty keyboard
/// protocol. Empty for a key that encoding has no bytes for, such as Caps Lock.
fn legacy_bytes(key: &KeyWithModifier) -> Vec<u8> {
    let has = |modifier| key.key_modifiers.contains(&modifier);
    let (ctrl, alt, shift) = (
        has(KeyModifier::Ctrl),
        has(KeyModifier::Alt),
        has(KeyModifier::Shift),
    );
    // xterm's modifier parameter: 1, plus 1 for Shift, 2 for Alt, 4 for Ctrl.
    let parameter = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let cursor = |letter: char| {
        if parameter == 1 {
            format!("\x1b[{letter}").into_bytes()
        } else {
            format!("\x1b[1;{parameter}{letter}").into_bytes()
        }
    };
    let tilde = |number: u8| {
        if parameter == 1 {
            format!("\x1b[{number}~").into_bytes()
        } else {
            format!("\x1b[{number};{parameter}~").into_bytes()
        }
    };
    // Keys sent as a single byte take Alt as an Escape in front of them.
    let with_alt = |bytes: Vec<u8>| {
        if alt {
            [vec![ESC], bytes].concat()
        } else {
            bytes
        }
    };

    match key.bare_key {
        BareKey::Char(character) => {
            let character = if shift {
                character.to_ascii_uppercase()
            } else {
                character
            };
            let bytes = match control_byte(character) {
                Some(byte) if ctrl => vec![byte],
                _ => character.to_string().into_bytes(),
            };
            with_alt(bytes)
        }
        BareKey::Enter => with_alt(vec![b'\r']),
        BareKey::Tab if shift => b"\x1b[Z".to_vec(),
        BareKey::Tab => with_alt(vec![b'\t']),
        BareKey::Esc => with_alt(vec![ESC]),
        BareKey::Backspace if ctrl => with_alt(vec![0x08]),
        BareKey::Backspace => with_alt(vec![0x7f]),
        BareKey::Up => cursor('A'),
        BareKey::Down => cursor('B'),
        BareKey::Right => cursor('C'),
        BareKey::Left => cursor('D'),
        BareKey::Home => cursor('H'),
        BareKey::End => cursor('F'),
        BareKey::Insert => tilde(2),
        BareKey::Delete => tilde(3),
        BareKey::PageUp => tilde(5),
        BareKey::PageDown => tilde(6),
        BareKey::F(number @ 1..=4) => {
            let letter = char::from(b'P' + number - 1);
            if parameter == 1 {
                format!("\x1bO{letter}").into_bytes()
            } else {
                cursor(letter)
            }
        }
        BareKey::F(number) => match number {
            5 => tilde(15),
            6..=10 => tilde(number + 11),
            11 | 12 => tilde(number + 12),
            _ => vec![],
        },
        BareKey::CapsLock
        | BareKey::ScrollLock
        | BareKey::NumLock
        | BareKey::PrintScreen
        | BareKey::Pause
        | BareKey::Menu => vec![],
    }
}

/// The byte Ctrl turns `character` into, for the characters it has one for.
fn control_byte(character: char) -> Option<u8> {
    match character {
        'a'..='z' | 'A'..='Z' => Some(character.to_ascii_lowercase() as u8 & 0x1f),
        '@' | ' ' | '2' => Some(0x00),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn key(text: &str) -> KeyWithModifier {
        KeyWithModifier::from_str(text).unwrap()
    }

    fn bytes(text: &str) -> Vec<u8> {
        legacy_bytes(&key(text))
    }

    #[test]
    fn plain_characters_are_themselves() {
        assert_eq!(bytes("a"), b"a");
        assert_eq!(bytes("]"), b"]");
    }

    #[test]
    fn ctrl_letters_are_control_bytes() {
        assert_eq!(bytes("Ctrl o"), vec![0x0f]);
        assert_eq!(bytes("Ctrl g"), vec![0x07]);
        assert_eq!(bytes("Ctrl ["), vec![ESC]);
    }

    #[test]
    fn alt_puts_an_escape_in_front() {
        assert_eq!(bytes("Alt n"), b"\x1bn");
        assert_eq!(bytes("Alt Ctrl o"), vec![ESC, 0x0f]);
    }

    #[test]
    fn named_keys_use_their_sequences() {
        assert_eq!(bytes("Enter"), b"\r");
        assert_eq!(bytes("Esc"), vec![ESC]);
        assert_eq!(bytes("Left"), b"\x1b[D");
        assert_eq!(bytes("PageDown"), b"\x1b[6~");
        assert_eq!(bytes("F1"), b"\x1bOP");
        assert_eq!(bytes("F5"), b"\x1b[15~");
        assert_eq!(bytes("F12"), b"\x1b[24~");
    }

    #[test]
    fn modified_cursor_keys_carry_the_modifier() {
        assert_eq!(bytes("Alt Left"), b"\x1b[1;3D");
        assert_eq!(bytes("Ctrl Shift Up"), b"\x1b[1;6A");
        assert_eq!(bytes("Alt Delete"), b"\x1b[3;3~");
    }

    #[test]
    fn keys_without_bytes_are_empty() {
        assert_eq!(bytes("CapsLock"), b"");
    }

    #[test]
    fn presses_carry_the_key_for_kitty_panes() {
        let keys = [key("Ctrl o"), key("d")];
        let actions = press_keys(&keys);
        assert_eq!(
            actions,
            vec![
                Action::Write {
                    key_with_modifier: Some(key("Ctrl o")),
                    bytes: vec![0x0f],
                    is_kitty_keyboard_protocol: false,
                },
                Action::Write {
                    key_with_modifier: Some(key("d")),
                    bytes: b"d".to_vec(),
                    is_kitty_keyboard_protocol: false,
                },
            ]
        );
    }
}
