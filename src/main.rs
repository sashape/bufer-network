//! BuferNet — обмен буфером и файлами по локальной сети.
//! Rust-версия: голый Win32 + Direct2D, без UI-библиотек.

#![windows_subsystem = "windows"]

mod autostart;
mod clipboard;
mod hotkeys;
mod config;
mod discovery;
mod events;
mod i18n;
mod icon;
mod json;
mod transfer;
mod ui;
mod util;

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

/// Вторая копия не запускается, а показывает окно работающей.
/// Мьютекс живёт до конца процесса (хэндл намеренно не закрывается).
fn another_instance_running() -> bool {
    if std::env::var_os("BUFERNET_ALLOW_MULTI").is_some() {
        return false; // лазейка для отладки и тестов
    }
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\BuferNet-SingleInstance"));
        GetLastError() == ERROR_ALREADY_EXISTS
    }
}

fn activate_existing_window() {
    unsafe {
        if let Ok(hwnd) = FindWindowW(w!("BuferNetWindow"), None) {
            let _ = PostMessageW(hwnd, ui::WM_APP_SHOW, WPARAM(0), LPARAM(0));
        }
    }
}

fn main() {
    if another_instance_running() {
        activate_existing_window();
        return;
    }
    unsafe {
        // до создания окна, чтобы оно не размывалось при масштабе 125/150%
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    }

    let server = match transfer::Server::bind() {
        Ok(s) => s,
        Err(e) => {
            // без приёмного сокета программа бессмысленна
            events::log(format!("fatal: cannot bind TCP socket: {e}"));
            return;
        }
    };
    let port = server.port;
    server.start();

    let disco = discovery::Discovery::new(port);
    disco.start();

    ui::run(disco, port);
}
