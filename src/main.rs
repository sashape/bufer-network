//! BuferNet — обмен буфером и файлами по локальной сети.
//! Rust-версия: голый Win32 + Direct2D, без UI-библиотек.

#![windows_subsystem = "windows"]

mod clipboard;
mod config;
mod discovery;
mod events;
mod i18n;
mod icon;
mod json;
mod transfer;
mod ui;

use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

fn main() {
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
