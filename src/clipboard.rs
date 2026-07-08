//! Чтение и запись текста в буфер обмена Windows (CF_UNICODETEXT).

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

fn open() -> Option<ClipboardGuard> {
    // буфер может держать другой процесс — пробуем несколько раз
    for _ in 0..5 {
        if unsafe { OpenClipboard(HWND::default()) }.is_ok() {
            return Some(ClipboardGuard);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    None
}

pub fn get_text() -> Option<String> {
    let _guard = open()?;
    unsafe {
        let handle: HANDLE = GetClipboardData(CF_UNICODETEXT).ok()?;
        let hglobal = HGLOBAL(handle.0);
        let ptr = GlobalLock(hglobal) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        let _ = GlobalUnlock(hglobal);
        Some(text)
    }
}

pub fn set_text(text: &str) -> bool {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let Some(_guard) = open() else {
        return false;
    };
    unsafe {
        if EmptyClipboard().is_err() {
            return false;
        }
        let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) else {
            return false;
        };
        let ptr = GlobalLock(hglobal) as *mut u16;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(hglobal);
        // после SetClipboardData память принадлежит системе
        SetClipboardData(CF_UNICODETEXT, HANDLE(hglobal.0)).is_ok()
    }
}
