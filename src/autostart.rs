//! Автозапуск при входе в Windows: значение в HKCU\...\Run.
//! Тем же ключом управляет MSI-установщик — источник истины всегда реестр.

use windows::core::w;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    RRF_RT_REG_SZ,
};

const RUN_KEY: windows::core::PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: windows::core::PCWSTR = w!("BuferNet");

pub fn enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
    }
}

pub fn set(enable: bool) -> bool {
    unsafe {
        if enable {
            let Ok(exe) = std::env::current_exe() else {
                return false;
            };
            let cmd = format!("\"{}\"", exe.display());
            let wide: Vec<u16> = cmd.encode_utf16().chain([0]).collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(wide.as_ptr() as _),
                (wide.len() * 2) as u32,
            )
            .is_ok()
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE).is_ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let was = enabled();
        assert!(set(true));
        assert!(enabled());
        assert!(set(false));
        assert!(!enabled());
        if was {
            set(true); // возвращаем как было
        }
    }
}
