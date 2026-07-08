//! Системные диалоги: сообщения, выбор файлов и папки.

use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH, FOS_ALLOWMULTISELECT,
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_YESNO,
};

use crate::config;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn info(hwnd: HWND, text: &str) {
    let text = wide(text);
    let title = wide(config::APP_NAME);
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_ICONINFORMATION,
        );
    }
}

pub fn confirm(hwnd: HWND, text: &str) -> bool {
    let text = wide(text);
    let title = wide(config::APP_NAME);
    unsafe {
        MessageBoxW(
            hwnd,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    }
}

fn shell_item_path(item: &windows::Win32::UI::Shell::IShellItem) -> Option<PathBuf> {
    unsafe {
        let raw = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = raw.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(raw.0 as _));
        path
    }
}

/// Мультивыбор файлов. Пустой список = отмена.
pub fn pick_files(hwnd: HWND, title: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    unsafe {
        let dialog: IFileOpenDialog = match CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL) {
            Ok(d) => d,
            Err(_) => return out,
        };
        let opts = dialog.GetOptions().unwrap_or_default();
        let _ = dialog.SetOptions(opts | FOS_ALLOWMULTISELECT | FOS_FORCEFILESYSTEM);
        let t = wide(title);
        let _ = dialog.SetTitle(PCWSTR(t.as_ptr()));
        if dialog.Show(hwnd).is_err() {
            return out; // отмена
        }
        let Ok(items) = dialog.GetResults() else {
            return out;
        };
        let count = items.GetCount().unwrap_or(0);
        for i in 0..count {
            if let Ok(item) = items.GetItemAt(i) {
                if let Some(p) = shell_item_path(&item) {
                    out.push(p);
                }
            }
        }
    }
    out
}

pub fn pick_folder(hwnd: HWND, title: &str) -> Option<PathBuf> {
    unsafe {
        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
        let opts = dialog.GetOptions().unwrap_or_default();
        let _ = dialog.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM);
        let t = wide(title);
        let _ = dialog.SetTitle(PCWSTR(t.as_ptr()));
        dialog.Show(hwnd).ok()?;
        let item = dialog.GetResult().ok()?;
        shell_item_path(&item)
    }
}
