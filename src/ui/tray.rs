//! Иконка в области уведомлений и всплывающие уведомления.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{HICON, WM_APP};

use crate::config;

pub const TRAY_MSG: u32 = WM_APP + 1;
const TRAY_ID: u32 = 1;

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    added: bool,
}

fn fill_wstr(dst: &mut [u16], s: &str) {
    let mut i = 0;
    for u in s.encode_utf16() {
        if i >= dst.len() - 1 {
            break;
        }
        dst[i] = u;
        i += 1;
    }
    dst[i] = 0;
}

impl Tray {
    pub fn new(hwnd: HWND, icon: HICON) -> Tray {
        Tray { hwnd, icon, added: false }
    }

    fn base(&self) -> NOTIFYICONDATAW {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: TRAY_ID,
            ..Default::default()
        };
        fill_wstr(&mut nid.szTip, config::APP_NAME);
        nid
    }

    pub fn add(&mut self) {
        let mut nid = self.base();
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.hIcon = self.icon;
        nid.uCallbackMessage = TRAY_MSG;
        self.added = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool();
    }

    /// Всплывающее уведомление возле трея. Не критично — ошибки глотаем.
    pub fn notify(&self, msg: &str) {
        if !self.added {
            return;
        }
        let mut nid = self.base();
        nid.uFlags = NIF_INFO;
        nid.dwInfoFlags = NIIF_INFO;
        fill_wstr(&mut nid.szInfo, msg);
        fill_wstr(&mut nid.szInfoTitle, config::APP_NAME);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }
    }

    pub fn remove(&mut self) {
        if self.added {
            let nid = self.base();
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            self.added = false;
        }
    }
}
