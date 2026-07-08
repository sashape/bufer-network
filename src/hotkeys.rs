//! Глобальные горячие клавиши: разбор строк вида "ctrl+alt+b",
//! человекочитаемое отображение и регистрация в системе.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

pub const ID_CLIP: i32 = 1;
pub const ID_FILES: i32 = 2;

#[derive(Clone, Copy, PartialEq)]
pub struct Combo {
    pub mods: u32, // MOD_CONTROL | MOD_ALT | ...
    pub vk: u32,
}

/// Спец-клавиши, у которых имя длиннее одного символа (кроме F1-F24).
const NAMED: &[(&str, u32)] = &[
    ("space", 0x20),
    ("pgup", 0x21),
    ("pgdn", 0x22),
    ("end", 0x23),
    ("home", 0x24),
    ("left", 0x25),
    ("up", 0x26),
    ("right", 0x27),
    ("down", 0x28),
    ("ins", 0x2D),
    ("del", 0x2E),
];

fn vk_from_name(name: &str) -> Option<u32> {
    if name.len() == 1 {
        let c = name.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() {
            return Some(c as u32);
        }
        return None;
    }
    if let Some(n) = name.strip_prefix('f').and_then(|d| d.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x6F + n);
        }
    }
    NAMED.iter().find(|(k, _)| *k == name).map(|(_, v)| *v)
}

fn name_from_vk(vk: u32) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => Some((vk as u8 as char).to_ascii_lowercase().to_string()),
        0x70..=0x87 => Some(format!("f{}", vk - 0x6F)),
        _ => NAMED.iter().find(|(_, v)| *v == vk).map(|(k, _)| k.to_string()),
    }
}

/// "ctrl+alt+b" -> Combo. Пустая строка или мусор -> None (хоткей выключен).
pub fn parse(s: &str) -> Option<Combo> {
    let mut mods = 0u32;
    let mut vk = None;
    for token in s.split('+').map(str::trim).filter(|t| !t.is_empty()) {
        match token.to_ascii_lowercase().as_str() {
            "ctrl" => mods |= MOD_CONTROL.0,
            "alt" => mods |= MOD_ALT.0,
            "shift" => mods |= MOD_SHIFT.0,
            "win" => mods |= MOD_WIN.0,
            key => vk = vk_from_name(key),
        }
    }
    // без ctrl/alt/win глобальный хоткей мешал бы обычному вводу
    if mods & (MOD_CONTROL.0 | MOD_ALT.0 | MOD_WIN.0) == 0 {
        return None;
    }
    vk.map(|vk| Combo { mods, vk })
}

/// Combo -> строка для настроек ("ctrl+alt+b").
pub fn serialize(c: Combo) -> String {
    let mut parts = Vec::new();
    if c.mods & MOD_CONTROL.0 != 0 {
        parts.push("ctrl".to_string());
    }
    if c.mods & MOD_SHIFT.0 != 0 {
        parts.push("shift".to_string());
    }
    if c.mods & MOD_ALT.0 != 0 {
        parts.push("alt".to_string());
    }
    if c.mods & MOD_WIN.0 != 0 {
        parts.push("win".to_string());
    }
    parts.push(name_from_vk(c.vk).unwrap_or_else(|| format!("0x{:x}", c.vk)));
    parts.join("+")
}

/// Combo -> подпись для меню ("Ctrl+Alt+B").
pub fn display(c: Combo) -> String {
    serialize(c)
        .split('+')
        .map(|p| {
            let mut ch = p.chars();
            match ch.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + ch.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

pub fn register(hwnd: HWND, id: i32, c: Combo) -> bool {
    unsafe {
        RegisterHotKey(
            hwnd,
            id,
            HOT_KEY_MODIFIERS(c.mods | MOD_NOREPEAT.0),
            c.vk,
        )
        .is_ok()
    }
}

pub fn unregister(hwnd: HWND, id: i32) {
    unsafe {
        let _ = UnregisterHotKey(hwnd, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_back() {
        let c = parse("ctrl+alt+b").unwrap();
        assert_eq!(c.mods, MOD_CONTROL.0 | MOD_ALT.0);
        assert_eq!(c.vk, 'B' as u32);
        assert_eq!(serialize(c), "ctrl+alt+b");
        assert_eq!(display(c), "Ctrl+Alt+B");
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!(parse("ctrl+f5").unwrap().vk, 0x74);
        assert_eq!(parse("win+shift+space").unwrap().vk, 0x20);
        assert_eq!(serialize(parse("ctrl+del").unwrap()), "ctrl+del");
    }

    #[test]
    fn rejects_weak_or_empty() {
        assert!(parse("").is_none());
        assert!(parse("b").is_none()); // без модификаторов
        assert!(parse("shift+b").is_none()); // только shift — мешал бы вводу
        assert!(parse("ctrl+").is_none());
    }
}
