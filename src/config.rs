//! Общие настройки приложения (порт Python-версии bufernet/config.py).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::json::{self, JVal};

pub const APP_NAME: &str = "BuferNet";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// UDP-порт, на котором компьютеры объявляют о себе в локальной сети
pub const DISCOVERY_PORT: u16 = 48765;
/// TCP-порт для приёма буфера и файлов (если занят — возьмётся свободный)
pub const TRANSFER_PORT: u16 = 48766;

pub const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(3);
pub const PEER_TIMEOUT: Duration = Duration::from_secs(10);

/// Максимальный размер текста буфера обмена (защита от мусора), байт
pub const MAX_CLIPBOARD_SIZE: u64 = 16 * 1024 * 1024;
/// Максимальный размер картинки из буфера (несжатый DIB), байт
pub const MAX_IMAGE_SIZE: u64 = 64 * 1024 * 1024;

pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn settings_file() -> PathBuf {
    home_dir().join(".bufernet.json")
}

pub fn default_downloads_dir() -> PathBuf {
    home_dir().join("Downloads").join(APP_NAME)
}

fn downloads_cell() -> &'static Mutex<PathBuf> {
    static CELL: OnceLock<Mutex<PathBuf>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(default_downloads_dir()))
}

pub fn downloads_dir() -> PathBuf {
    downloads_cell().lock().unwrap().clone()
}

pub fn set_downloads_dir(p: PathBuf) {
    *downloads_cell().lock().unwrap() = p;
}

/// "1.2.10" -> [1, 2, 10] для сравнения версий; мусор считается самой старой.
pub fn version_tuple(v: &str) -> Vec<u64> {
    let parts: Option<Vec<u64>> = v.split('.').map(|x| x.parse().ok()).collect();
    match parts {
        Some(p) if !p.is_empty() => p,
        _ => vec![0],
    }
}

/// Пользовательские настройки (~/.bufernet.json, совместим с Python-версией).
#[derive(Default, Clone)]
pub struct Settings {
    pub theme: String,        // "auto" / "light" / "dark"
    pub lang: String,         // "auto" / код из i18n
    pub download_dir: String, // пустая = по умолчанию
    pub hotkey_clip: String,  // "ctrl+alt+b"; пустая = выключено
    pub hotkey_files: String, // "ctrl+alt+f"; пустая = выключено
}

pub fn load_settings() -> Settings {
    let mut s = Settings {
        theme: "auto".into(),
        lang: "auto".into(),
        download_dir: String::new(),
        hotkey_clip: "ctrl+alt+b".into(),
        hotkey_files: "ctrl+alt+f".into(),
    };
    let Ok(text) = std::fs::read_to_string(settings_file()) else {
        return s;
    };
    // некоторые редакторы сохраняют файл с UTF-8 BOM — не даём ему сорвать разбор
    let text = text.trim_start_matches('\u{feff}');
    let Some(map) = json::parse_object(text) else {
        return s;
    };
    let get = |k: &str| -> Option<String> {
        map.get(k).and_then(JVal::as_str).map(str::to_owned)
    };
    if let Some(v) = get("theme") {
        s.theme = v;
    }
    if let Some(v) = get("lang") {
        s.lang = v;
    }
    if let Some(v) = get("download_dir") {
        s.download_dir = v;
    }
    // ключ присутствует, но пуст — хоткей осознанно выключен;
    // ключа нет (старые настройки) — остаётся значение по умолчанию
    if let Some(v) = get("hotkey_clip") {
        s.hotkey_clip = v;
    }
    if let Some(v) = get("hotkey_files") {
        s.hotkey_files = v;
    }
    s
}

pub fn save_settings(s: &Settings) {
    let mut pairs: Vec<(&str, String)> = vec![
        ("theme", json::quote(&s.theme)),
        ("lang", json::quote(&s.lang)),
        ("hotkey_clip", json::quote(&s.hotkey_clip)),
        ("hotkey_files", json::quote(&s.hotkey_files)),
    ];
    if !s.download_dir.is_empty() {
        pairs.push(("download_dir", json::quote(&s.download_dir)));
    }
    let _ = std::fs::write(settings_file(), json::object(&pairs)); // настройки не критичны
}
