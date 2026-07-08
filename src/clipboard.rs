//! Чтение и запись буфера обмена Windows: текст (CF_UNICODETEXT)
//! и картинки (CF_DIB — заголовок BITMAPINFO + пиксели, без сжатия).

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};

const CF_UNICODETEXT: u32 = 13;
const CF_DIB: u32 = 8;

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

/// Картинка из буфера как сырые байты DIB (CF_DIB синтезируется системой
/// и из CF_BITMAP, так что скриншоты и копии из браузера тоже видны).
pub fn get_dib() -> Option<Vec<u8>> {
    let _guard = open()?;
    unsafe {
        let handle: HANDLE = GetClipboardData(CF_DIB).ok()?;
        let hglobal = HGLOBAL(handle.0);
        let size = GlobalSize(hglobal);
        if size == 0 {
            return None;
        }
        let ptr = GlobalLock(hglobal) as *const u8;
        if ptr.is_null() {
            return None;
        }
        let data = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(hglobal);
        Some(data)
    }
}

pub fn set_dib(data: &[u8]) -> bool {
    let Some(_guard) = open() else {
        return false;
    };
    unsafe {
        if EmptyClipboard().is_err() {
            return false;
        }
        let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, data.len()) else {
            return false;
        };
        let ptr = GlobalLock(hglobal) as *mut u8;
        if ptr.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
        let _ = GlobalUnlock(hglobal);
        SetClipboardData(CF_DIB, HANDLE(hglobal.0)).is_ok()
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
