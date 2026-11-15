use gtk::gdk::{Key, ModifierType};

pub fn encode(key: Key, modifiers: ModifierType, application_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = modifiers.contains(ModifierType::CONTROL_MASK);
    let alt = modifiers.contains(ModifierType::ALT_MASK);
    let shift = modifiers.contains(ModifierType::SHIFT_MASK);
    let name = key.name()?;
    let modifier = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let cursor = match name.as_str() {
        "Up" => Some('A'),
        "Down" => Some('B'),
        "Right" => Some('C'),
        "Left" => Some('D'),
        "Home" => Some('H'),
        "End" => Some('F'),
        _ => None,
    };
    if let Some(code) = cursor {
        return Some(
            if modifier > 1 {
                format!("\x1b[1;{modifier}{code}")
            } else if application_cursor {
                format!("\x1bO{code}")
            } else {
                format!("\x1b[{code}")
            }
            .into_bytes(),
        );
    }
    let tilde = match name.as_str() {
        "Insert" => Some(2),
        "Delete" => Some(3),
        "Page_Up" => Some(5),
        "Page_Down" => Some(6),
        "F5" => Some(15),
        "F6" => Some(17),
        "F7" => Some(18),
        "F8" => Some(19),
        "F9" => Some(20),
        "F10" => Some(21),
        "F11" => Some(23),
        "F12" => Some(24),
        _ => None,
    };
    if let Some(code) = tilde {
        return Some(
            if modifier > 1 {
                format!("\x1b[{code};{modifier}~")
            } else {
                format!("\x1b[{code}~")
            }
            .into_bytes(),
        );
    }
    let mut bytes = match name.as_str() {
        "Return" | "KP_Enter" => vec![b'\r'],
        "BackSpace" => vec![127],
        "Tab" if !shift => vec![b'\t'],
        "ISO_Left_Tab" | "Tab" => b"\x1b[Z".to_vec(),
        "Escape" => vec![27],
        "F1" | "F2" | "F3" | "F4" => {
            let code = (b'P' + name.as_bytes()[1] - b'1') as char;
            return Some(
                if modifier > 1 {
                    format!("\x1b[1;{modifier}{code}")
                } else {
                    format!("\x1bO{code}")
                }
                .into_bytes(),
            );
        }
        _ if ctrl => {
            let c = key.to_unicode()?.to_ascii_uppercase();
            if c == ' ' {
                vec![0]
            } else if ('@'..='_').contains(&c) {
                vec![c as u8 & 0x1f]
            } else {
                return None;
            }
        }
        _ if alt => key.to_unicode()?.to_string().into_bytes(),
        _ => return None, // Normal Unicode/dead keys/IME go through IMContext commit.
    };
    if alt {
        bytes.insert(0, 27);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encodes_control_and_application_keys_without_eating_unicode_ime() {
        assert_eq!(
            encode(Key::c, ModifierType::CONTROL_MASK, false).unwrap(),
            b"\x03"
        );
        assert_eq!(
            encode(Key::Left, ModifierType::empty(), true).unwrap(),
            b"\x1bOD"
        );
        assert_eq!(
            encode(Key::Up, ModifierType::CONTROL_MASK, false).unwrap(),
            b"\x1b[1;5A"
        );
        assert_eq!(
            encode(Key::F1, ModifierType::empty(), false).unwrap(),
            b"\x1bOP"
        );
        assert!(encode(Key::eacute, ModifierType::empty(), false).is_none());
        assert_eq!(
            encode(Key::Return, ModifierType::empty(), false).unwrap(),
            b"\r"
        );
    }
}
