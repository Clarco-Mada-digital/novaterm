//! Encodage des touches en séquences d'échappement terminal (xterm).

use egui::{Key, Modifiers};

fn modifier_code(mods: &Modifiers) -> u8 {
    1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.ctrl)
}

fn cursor_key(final_char: u8, app_cursor: bool, mods: &Modifiers) -> Vec<u8> {
    let m = 1 + u8::from(mods.shift) + 4 * u8::from(mods.ctrl);
    if m > 1 {
        format!("\x1b[1;{m}{}", final_char as char).into_bytes()
    } else if app_cursor {
        vec![0x1b, b'O', final_char]
    } else {
        vec![0x1b, b'[', final_char]
    }
}

fn tilde_key(code: u8, mods: &Modifiers) -> Vec<u8> {
    let m = modifier_code(mods);
    if m > 1 {
        format!("\x1b[{code};{m}~").into_bytes()
    } else {
        format!("\x1b[{code}~").into_bytes()
    }
}

fn letter(key: Key) -> Option<char> {
    let c = match key {
        Key::A => 'a', Key::B => 'b', Key::C => 'c', Key::D => 'd', Key::E => 'e',
        Key::F => 'f', Key::G => 'g', Key::H => 'h', Key::I => 'i', Key::J => 'j',
        Key::K => 'k', Key::L => 'l', Key::M => 'm', Key::N => 'n', Key::O => 'o',
        Key::P => 'p', Key::Q => 'q', Key::R => 'r', Key::S => 's', Key::T => 't',
        Key::U => 'u', Key::V => 'v', Key::W => 'w', Key::X => 'x', Key::Y => 'y',
        Key::Z => 'z',
        _ => return None,
    };
    Some(c)
}

/// Séquence pour une touche spéciale ou une combinaison Ctrl/Alt.
/// Les caractères imprimables simples arrivent via `Event::Text`.
pub fn encode_key(key: Key, mods: &Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let base: Option<Vec<u8>> = match key {
        Key::ArrowUp => Some(cursor_key(b'A', app_cursor, mods)),
        Key::ArrowDown => Some(cursor_key(b'B', app_cursor, mods)),
        Key::ArrowRight => Some(cursor_key(b'C', app_cursor, mods)),
        Key::ArrowLeft => Some(cursor_key(b'D', app_cursor, mods)),
        Key::Home => Some(cursor_key(b'H', app_cursor, mods)),
        Key::End => Some(cursor_key(b'F', app_cursor, mods)),
        Key::Insert => Some(tilde_key(2, mods)),
        Key::Delete => Some(tilde_key(3, mods)),
        Key::PageUp => Some(tilde_key(5, mods)),
        Key::PageDown => Some(tilde_key(6, mods)),
        Key::Enter => Some(b"\r".to_vec()),
        Key::Tab => Some(if mods.shift { b"\x1b[Z".to_vec() } else { b"\t".to_vec() }),
        Key::Backspace => Some(if mods.ctrl { vec![0x08] } else { vec![0x7f] }),
        Key::Escape => Some(vec![0x1b]),
        Key::F1 => Some(f_key(b'P', mods)),
        Key::F2 => Some(f_key(b'Q', mods)),
        Key::F3 => Some(f_key(b'R', mods)),
        Key::F4 => Some(f_key(b'S', mods)),
        Key::F5 => Some(tilde_key(15, mods)),
        Key::F6 => Some(tilde_key(17, mods)),
        Key::F7 => Some(tilde_key(18, mods)),
        Key::F8 => Some(tilde_key(19, mods)),
        Key::F9 => Some(tilde_key(20, mods)),
        Key::F10 => Some(tilde_key(21, mods)),
        Key::F11 => Some(tilde_key(23, mods)),
        Key::F12 => Some(tilde_key(24, mods)),
        _ => {
            if mods.ctrl {
                ctrl_byte(key, mods)
            } else {
                None
            }
        }
    };

    match base {
        Some(mut v) => {
            if mods.alt && key != Key::Escape {
                v.insert(0, 0x1b);
            }
            Some(v)
        }
        None => {
            // Alt + lettre -> ESC préfixe (Mode_switch / Meta).
            if mods.alt && !mods.ctrl {
                if let Some(c) = letter(key) {
                    let c = if mods.shift { c.to_ascii_uppercase() } else { c };
                    return Some(vec![0x1b, c as u8]);
                }
            }
            None
        }
    }
}

fn f_key(final_char: u8, mods: &Modifiers) -> Vec<u8> {
    let m = modifier_code(mods);
    if m > 1 {
        format!("\x1b[1;{m}{}", final_char as char).into_bytes()
    } else {
        vec![0x1b, b'O', final_char]
    }
}

fn ctrl_byte(key: Key, mods: &Modifiers) -> Option<Vec<u8>> {
    let b = match key {
        Key::Space => 0x00,
        Key::OpenBracket => 0x1b,
        Key::Backslash => 0x1c,
        Key::CloseBracket => 0x1d,
        Key::Minus | Key::Slash => 0x1f,
        Key::Num2 => 0x00,
        Key::Num3 => 0x1b,
        Key::Num4 => 0x1c,
        Key::Num5 => 0x1d,
        Key::Num6 => 0x1e,
        Key::Num7 => 0x1f,
        Key::Num8 => 0x7f,
        _ => {
            let c = letter(key)?;
            (c.to_ascii_uppercase() as u8) & 0x1f
        }
    };
    let _ = mods;
    Some(vec![b])
}

/// Boutons de souris au sens xterm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    /// Mouvement sans bouton enfoncé.
    None,
}

/// Encode un événement souris (mode SGR 1006 ou X10 classique).
/// `col` et `line` sont en coordonnées écran (0-based).
pub fn mouse_report(
    button: MouseButton,
    col: usize,
    line: usize,
    pressed: bool,
    motion: bool,
    mods: &Modifiers,
    sgr: bool,
) -> Option<Vec<u8>> {
    let mut code: u8 = match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::None => 3,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
    };
    if mods.shift {
        code += 4;
    }
    if mods.alt {
        code += 8;
    }
    if mods.ctrl {
        code += 16;
    }
    if motion {
        code += 32;
    }
    if sgr {
        let suffix = if pressed { 'M' } else { 'm' };
        Some(format!("\x1b[<{code};{};{}{suffix}", col + 1, line + 1).into_bytes())
    } else {
        // X10 : le relâchement est signalé par le code 3, coordonnées limitées à 222.
        if col > 222 || line > 222 {
            return None;
        }
        let code = if !pressed && !matches!(button, MouseButton::WheelUp | MouseButton::WheelDown) {
            (code & !3) | 3
        } else {
            code
        };
        Some(vec![0x1b, b'[', b'M', 32 + code, 33 + col as u8, 33 + line as u8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mods(ctrl: bool, alt: bool, shift: bool) -> Modifiers {
        Modifiers { ctrl, alt, shift, ..Default::default() }
    }

    #[test]
    fn ctrl_c_is_sigint() {
        assert_eq!(encode_key(Key::C, &mods(true, false, false), false), Some(vec![3]));
    }

    #[test]
    fn arrows_normal_and_app() {
        assert_eq!(encode_key(Key::ArrowUp, &Modifiers::default(), false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode_key(Key::ArrowUp, &Modifiers::default(), true), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn ctrl_arrow() {
        assert_eq!(
            encode_key(Key::ArrowRight, &mods(true, false, false), false),
            Some(b"\x1b[1;5C".to_vec())
        );
    }

    #[test]
    fn alt_letter() {
        assert_eq!(encode_key(Key::X, &mods(false, true, false), false), Some(vec![0x1b, b'x']));
    }

    #[test]
    fn shift_tab() {
        assert_eq!(encode_key(Key::Tab, &mods(false, false, true), false), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn f5_and_delete() {
        assert_eq!(encode_key(Key::F5, &Modifiers::default(), false), Some(b"\x1b[15~".to_vec()));
        assert_eq!(encode_key(Key::Delete, &Modifiers::default(), false), Some(b"\x1b[3~".to_vec()));
    }

    #[test]
    fn mouse_sgr_and_x10() {
        let m = Modifiers::default();
        assert_eq!(
            mouse_report(MouseButton::Left, 4, 9, true, false, &m, true),
            Some(b"\x1b[<0;5;10M".to_vec())
        );
        assert_eq!(
            mouse_report(MouseButton::Left, 4, 9, false, false, &m, true),
            Some(b"\x1b[<0;5;10m".to_vec())
        );
        assert_eq!(
            mouse_report(MouseButton::WheelUp, 0, 0, true, false, &m, false),
            Some(vec![0x1b, b'[', b'M', 32 + 64, 33, 33])
        );
    }

    #[test]
    fn ctrl_digits() {
        assert_eq!(encode_key(Key::Num2, &mods(true, false, false), false), Some(vec![0]));
        assert_eq!(encode_key(Key::Backspace, &mods(true, false, false), false), Some(vec![8]));
    }

    #[test]
    fn plain_letter_not_encoded() {
        assert_eq!(encode_key(Key::A, &Modifiers::default(), false), None);
    }
}