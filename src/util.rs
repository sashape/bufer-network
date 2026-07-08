//! Мелкие общие хелперы.

/// Строка → нуль-терминированный UTF-16 буфер для Win32 *W-функций.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
