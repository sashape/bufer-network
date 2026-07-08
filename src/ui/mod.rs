//! Главное окно: голый Win32 + собственная отрисовка (render.rs).

mod dialogs;
mod render;
mod tray;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, EndPaint, GetStockObject, InvalidateRect, ScreenToClient,
    HBRUSH, NULL_BRUSH, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

// в windows-rs лежит в UI::Controls — не тянем целую фичу ради одной константы
const WM_MOUSELEAVE: u32 = 0x02A3;
// клик по всплывающему уведомлению трея (shellapi.h)
const NIN_BALLOONUSERCLICK: u32 = 0x0405;

use crate::config;
use crate::discovery::{Discovery, Peer};
use crate::events::{self, UiEvent};
use crate::i18n::{self, tr, trf};
use crate::{clipboard, icon, transfer};

use render::{HAlign, Rect};
use tray::TRAY_MSG;

// команды меню
const CMD_THEME_AUTO: u32 = 110;
const CMD_THEME_LIGHT: u32 = 111;
const CMD_THEME_DARK: u32 = 112;
const CMD_LANG_AUTO: u32 = 120; // 121..=125 — языки по порядку
const CMD_ROLLOUT: u32 = 130;
const CMD_OPEN_FOLDER: u32 = 131;
const CMD_CHANGE_FOLDER: u32 = 132;
const CMD_AUTOSTART: u32 = 133;
const CMD_TRAY_SHOW: u32 = 140;
const CMD_TRAY_EXIT: u32 = 141;

const ROWS_VISIBLE: usize = 5;
const ROW_H: f32 = 38.0;

/// Что делать по клику на последнее уведомление в трее.
#[derive(Clone, PartialEq)]
enum NotifyAction {
    ShowWindow,
    OpenFile(PathBuf), // Проводник с выделенным файлом
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Hit {
    None,
    Gear,
    Peer(usize),
    BtnClip,
    BtnFiles,
    Log,
    LogBar,
}

struct Layout {
    gear: Rect,
    list: Rect,
    btn_clip: Rect,
    btn_files: Rect,
    log: Rect,
}

struct App {
    hwnd: HWND,
    scale: f32,
    dark: bool,
    settings: config::Settings,
    disco: Arc<Discovery>,
    server_port: u16,
    peers: Vec<Peer>,
    peers_sig: Option<Vec<String>>,
    selected: Option<String>,
    peer_scroll: usize,
    log: Vec<String>,
    log_scroll: f32,
    log_at_bottom: bool,
    log_view: (f32, f32), // (content_h, view_h) после последнего кадра
    log_bar: Option<Rect>,
    drag_bar: Option<f32>, // смещение точки захвата от верха ползунка
    hover: Hit,
    pressed: Option<Hit>,
    tracking_leave: bool,
    render: Option<render::Ctx>,
    tray: tray::Tray,
    notify_action: NotifyAction,
}

pub fn run(disco: Arc<Discovery>, server_port: u16) {
    let settings = config::load_settings();
    apply_language(&settings.lang);
    if !settings.download_dir.is_empty() {
        config::set_downloads_dir(PathBuf::from(&settings.download_dir));
    }

    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap().into();
        let class_name = w!("BuferNetWindow");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH(GetStockObject(NULL_BRUSH).0),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let dark = resolve_dark(&settings.theme);
        let mut boxed = Box::new(App {
            hwnd: HWND::default(),
            scale: 1.0,
            dark,
            settings,
            disco,
            server_port,
            peers: Vec::new(),
            peers_sig: None,
            selected: None,
            peer_scroll: 0,
            log: Vec::new(),
            log_scroll: 0.0,
            log_at_bottom: true,
            log_view: (0.0, 0.0),
            log_bar: None,
            drag_bar: None,
            hover: Hit::None,
            pressed: None,
            tracking_leave: false,
            render: None,
            tray: tray::Tray::new(HWND::default(), Default::default()),
            notify_action: NotifyAction::ShowWindow,
        });

        let title: Vec<u16> = config::APP_NAME.encode_utf16().chain([0]).collect();
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            440,
            560,
            None,
            None,
            hinstance,
            Some(&mut *boxed as *mut App as _),
        )
        .unwrap();

        // размер под реальный DPI монитора
        let dpi = GetDpiForWindow(hwnd) as f32;
        let s = dpi / 96.0;
        let _ = SetWindowPos(
            hwnd,
            HWND::default(),
            0,
            0,
            (440.0 * s) as i32,
            (560.0 * s) as i32,
            SWP_NOMOVE | SWP_NOZORDER,
        );

        // иконки окна и трея
        if let Some(big) = icon::create(32) {
            SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_BIG as usize), LPARAM(big.0 as isize));
        }
        if let Some(small) = icon::create(16) {
            SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_SMALL as usize), LPARAM(small.0 as isize));
            boxed.tray = tray::Tray::new(hwnd, small);
            boxed.tray.add();
        }

        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        // окно закрыто навсегда — гасим фоновые потоки вместе с процессом
        boxed.tray.remove();
    }
    std::process::exit(0);
}

fn apply_language(pref: &str) {
    if pref == "auto" {
        i18n::set_language(i18n::detect_system_language());
    } else {
        i18n::set_language(pref);
    }
}

/// Тёмная ли тема приложений в настройках Windows.
fn windows_is_dark() -> bool {
    let mut value: u32 = 1;
    let mut size = 4u32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as _),
            Some(&mut size),
        )
    };
    ok.is_ok() && value == 0
}

fn resolve_dark(pref: &str) -> bool {
    match pref {
        "light" => false,
        "dark" => true,
        _ => windows_is_dark(),
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = lparam.0 as *const CREATESTRUCTW;
            let app = (*cs).lpCreateParams as *mut App;
            (*app).hwnd = hwnd;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, app as isize);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let app = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
        if app.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        (*app).handle(msg, wparam, lparam)
    }
}

impl App {
    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe {
            match msg {
                WM_CREATE => {
                    self.scale = GetDpiForWindow(self.hwnd) as f32 / 96.0;
                    self.apply_titlebar();
                    SetTimer(self.hwnd, 1, 200, None);
                    events::log(trf("log_start", &[
                        ("app", config::APP_NAME),
                        ("version", config::VERSION),
                        ("name", &self.disco.my_name),
                        ("port", &self.server_port.to_string()),
                    ]));
                    events::log(trf("log_downloads", &[(
                        "dir",
                        &config::downloads_dir().display().to_string(),
                    )]));
                    LRESULT(0)
                }
                WM_TIMER => {
                    self.process_events();
                    self.refresh_peers();
                    LRESULT(0)
                }
                WM_PAINT => {
                    let mut ps = PAINTSTRUCT::default();
                    BeginPaint(self.hwnd, &mut ps);
                    self.draw();
                    let _ = EndPaint(self.hwnd, &ps);
                    LRESULT(0)
                }
                WM_ERASEBKGND => LRESULT(1),
                WM_SIZE => {
                    let (w, h) = (lparam.0 as u32 & 0xFFFF, (lparam.0 as u32 >> 16) & 0xFFFF);
                    if let Some(r) = &self.render {
                        r.resize(w, h);
                    }
                    self.invalidate();
                    LRESULT(0)
                }
                WM_GETMINMAXINFO => {
                    let mmi = lparam.0 as *mut MINMAXINFO;
                    (*mmi).ptMinTrackSize.x = (400.0 * self.scale) as i32;
                    (*mmi).ptMinTrackSize.y = (460.0 * self.scale) as i32;
                    LRESULT(0)
                }
                WM_DPICHANGED => {
                    self.scale = (wparam.0 as u32 & 0xFFFF) as f32 / 96.0;
                    if let Some(r) = &self.render {
                        r.set_dpi(self.scale * 96.0);
                    }
                    let rect = &*(lparam.0 as *const RECT);
                    let _ = SetWindowPos(
                        self.hwnd,
                        HWND::default(),
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER,
                    );
                    self.invalidate();
                    LRESULT(0)
                }
                WM_SETTINGCHANGE => {
                    // тема Windows могла смениться
                    if self.settings.theme == "auto" {
                        let dark = windows_is_dark();
                        if dark != self.dark {
                            self.dark = dark;
                            self.apply_titlebar();
                            self.invalidate();
                        }
                    }
                    LRESULT(0)
                }
                WM_MOUSEMOVE => {
                    let (x, y) = self.mouse_pos(lparam);
                    if !self.tracking_leave {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                        self.tracking_leave = true;
                    }
                    if let Some(grab) = self.drag_bar {
                        self.drag_log_bar(y, grab);
                    } else {
                        let hit = self.hit_test(x, y);
                        if hit != self.hover {
                            self.hover = hit;
                            self.invalidate();
                        }
                    }
                    LRESULT(0)
                }
                WM_MOUSELEAVE => {
                    self.tracking_leave = false;
                    if self.hover != Hit::None {
                        self.hover = Hit::None;
                        self.invalidate();
                    }
                    LRESULT(0)
                }
                WM_LBUTTONDOWN => {
                    let (x, y) = self.mouse_pos(lparam);
                    let hit = self.hit_test(x, y);
                    self.pressed = Some(hit);
                    SetCapture(self.hwnd);
                    if hit == Hit::LogBar {
                        if let Some(bar) = self.log_bar {
                            self.drag_bar = Some(y - bar.t);
                        }
                    }
                    self.invalidate();
                    LRESULT(0)
                }
                WM_LBUTTONUP => {
                    let (x, y) = self.mouse_pos(lparam);
                    let _ = ReleaseCapture();
                    self.drag_bar = None;
                    let hit = self.hit_test(x, y);
                    let pressed = self.pressed.take();
                    self.invalidate();
                    if pressed == Some(hit) {
                        self.click(hit);
                    }
                    LRESULT(0)
                }
                WM_MOUSEWHEEL => {
                    let delta = (wparam.0 >> 16) as i16 as f32 / 120.0;
                    // координаты колеса приходят в экранных
                    let mut pt = POINT {
                        x: lparam.0 as i16 as i32,
                        y: (lparam.0 >> 16) as i16 as i32,
                    };
                    let _ = ScreenToClient(self.hwnd, &mut pt);
                    let (x, y) = (pt.x as f32 / self.scale, pt.y as f32 / self.scale);
                    self.wheel(x, y, delta);
                    LRESULT(0)
                }
                WM_SETCURSOR => {
                    let clickable = matches!(
                        self.hover,
                        Hit::Gear | Hit::Peer(_) | Hit::BtnClip | Hit::BtnFiles | Hit::LogBar
                    );
                    if clickable {
                        SetCursor(LoadCursorW(None, IDC_HAND).unwrap_or_default());
                        return LRESULT(1);
                    }
                    DefWindowProcW(self.hwnd, msg, wparam, lparam)
                }
                WM_CLOSE => {
                    // закрытие окна — сворачивание в трей, приложение живёт
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                    self.notify(&tr("tray_minimized"), NotifyAction::ShowWindow);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    self.tray.remove();
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                TRAY_MSG => {
                    match (lparam.0 & 0xFFFF) as u32 {
                        WM_LBUTTONUP => self.show_window(),
                        WM_RBUTTONUP | WM_CONTEXTMENU => self.tray_menu(),
                        NIN_BALLOONUSERCLICK => match self.notify_action.clone() {
                            NotifyAction::ShowWindow => self.show_window(),
                            NotifyAction::OpenFile(path) => self.reveal_in_explorer(&path),
                        },
                        _ => {}
                    }
                    LRESULT(0)
                }
                _ => DefWindowProcW(self.hwnd, msg, wparam, lparam),
            }
        }
    }

    fn mouse_pos(&self, lparam: LPARAM) -> (f32, f32) {
        let x = lparam.0 as i16 as f32;
        let y = (lparam.0 >> 16) as i16 as f32;
        (x / self.scale, y / self.scale)
    }

    fn invalidate(&self) {
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn client_size(&self) -> (f32, f32) {
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        (
            (rc.right - rc.left) as f32 / self.scale,
            (rc.bottom - rc.top) as f32 / self.scale,
        )
    }

    /// Тёмный/светлый заголовок окна (Windows 11; на 10 просто не сработает).
    fn apply_titlebar(&self) {
        let dark: i32 = self.dark as i32;
        let caption: u32 = if self.dark { 0x001c1c1c } else { 0x00fafafa };
        unsafe {
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark as *const _ as _,
                4,
            );
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_CAPTION_COLOR,
                &caption as *const _ as _,
                4,
            );
        }
    }

    // --- геометрия ---

    fn layout(&self) -> Layout {
        let (w, h) = self.client_size();
        let pad = 16.0;
        let list_top = 84.0;
        let list = Rect::new(pad, list_top, w - pad, list_top + 8.0 + ROWS_VISIBLE as f32 * ROW_H);
        let btn_top = list.b + 14.0;
        let mid = w / 2.0;
        let btn_clip = Rect::new(pad, btn_top, mid - 4.0, btn_top + 36.0);
        let btn_files = Rect::new(mid + 4.0, btn_top, w - pad, btn_top + 36.0);
        let log_top = btn_top + 36.0 + 44.0;
        let log = Rect::new(pad, log_top, w - pad, (h - pad).max(log_top + 40.0));
        Layout {
            gear: Rect::new(w - pad - 36.0, 16.0, w - pad, 16.0 + 32.0),
            list,
            btn_clip,
            btn_files,
            log,
        }
    }

    fn hit_test(&self, x: f32, y: f32) -> Hit {
        let l = self.layout();
        if l.gear.contains(x, y) {
            return Hit::Gear;
        }
        if l.btn_clip.contains(x, y) {
            return Hit::BtnClip;
        }
        if l.btn_files.contains(x, y) {
            return Hit::BtnFiles;
        }
        if let Some(bar) = self.log_bar {
            // ползунок ловится с запасом по ширине
            if Rect::new(bar.l - 4.0, bar.t, bar.r + 4.0, bar.b).contains(x, y) {
                return Hit::LogBar;
            }
        }
        if l.list.contains(x, y) && !self.peers.is_empty() {
            let idx = ((y - l.list.t - 4.0) / ROW_H).floor() as isize + self.peer_scroll as isize;
            if idx >= 0 && (idx as usize) < self.peers.len() {
                return Hit::Peer(idx as usize);
            }
        }
        if l.log.contains(x, y) {
            return Hit::Log;
        }
        Hit::None
    }

    fn wheel(&mut self, x: f32, y: f32, delta: f32) {
        let l = self.layout();
        if l.log.contains(x, y) || matches!(self.hover, Hit::LogBar) {
            let (content, view) = self.log_view;
            let max = (content - view).max(0.0);
            self.log_scroll = (self.log_scroll - delta * 48.0).clamp(0.0, max);
            self.log_at_bottom = self.log_scroll >= max - 1.0;
            self.invalidate();
        } else if l.list.contains(x, y) {
            let max = self.peers.len().saturating_sub(ROWS_VISIBLE);
            let next = self.peer_scroll as isize - delta as isize;
            self.peer_scroll = next.clamp(0, max as isize) as usize;
            self.invalidate();
        }
    }

    fn drag_log_bar(&mut self, y: f32, grab: f32) {
        let (content, view) = self.log_view;
        let Some(bar) = self.log_bar else { return };
        let l = self.layout();
        let track_top = l.log.t + 4.0;
        let track_h = l.log.h() - 8.0;
        let bar_h = bar.h();
        let max_scroll = (content - view).max(0.0);
        if track_h <= bar_h || max_scroll <= 0.0 {
            return;
        }
        let ratio = ((y - grab) - track_top) / (track_h - bar_h);
        self.log_scroll = (ratio.clamp(0.0, 1.0)) * max_scroll;
        self.log_at_bottom = self.log_scroll >= max_scroll - 1.0;
        self.invalidate();
    }

    // --- события из фоновых потоков ---

    fn process_events(&mut self) {
        for ev in events::drain() {
            match ev {
                UiEvent::Log(msg) => self.push_log(msg),
                UiEvent::ClipboardReceived { text, sender } => {
                    clipboard::set_text(&text);
                    let mut preview: String =
                        text.trim().replace('\n', " ").chars().take(60).collect();
                    if text.trim().chars().count() > 60 {
                        preview.push('…');
                    }
                    self.push_log(trf("clip_received", &[
                        ("name", &sender),
                        ("preview", &preview),
                    ]));
                    self.notify(&trf("notify_clip", &[("name", &sender)]), NotifyAction::ShowWindow);
                }
                UiEvent::ImageReceived { data, sender } => {
                    clipboard::set_dib(&data);
                    self.push_log(trf("img_received", &[
                        ("name", &sender),
                        ("size", &transfer::fmt_size(data.len() as u64)),
                    ]));
                    self.notify(&trf("notify_img", &[("name", &sender)]), NotifyAction::ShowWindow);
                }
                UiEvent::FileReceived { path, sender } => {
                    self.push_log(trf("file_received", &[
                        ("name", &sender),
                        ("file", &path.display().to_string()),
                    ]));
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    self.notify(
                        &trf("notify_file", &[("file", &name), ("name", &sender)]),
                        NotifyAction::OpenFile(path),
                    );
                }
                UiEvent::UpdateReceived { path, version, sender } => {
                    self.on_update_received(path, version, sender);
                }
            }
        }
    }

    fn push_log(&mut self, msg: String) {
        self.log.push(msg);
        self.invalidate();
    }

    /// Уведомление в трее + что сделать, если по нему кликнут.
    fn notify(&mut self, msg: &str, action: NotifyAction) {
        self.notify_action = action;
        self.tray.notify(msg);
    }

    /// Открыть Проводник с выделенным файлом.
    fn reveal_in_explorer(&self, path: &Path) {
        // explorer /select молча игнорирует прямые слэши (а они приходят
        // из настроек, записанных Python-версией) — нормализуем
        let normalized = path.display().to_string().replace('/', "\\");
        let args = format!("/select,\"{normalized}\"");
        let wide: Vec<u16> = args.encode_utf16().chain([0]).collect();
        unsafe {
            ShellExecuteW(
                self.hwnd,
                w!("open"),
                w!("explorer.exe"),
                PCWSTR(wide.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }

    fn refresh_peers(&mut self) {
        let peers = self.disco.get_peers();
        let sig: Vec<String> = peers
            .iter()
            .map(|p| format!("{}|{}|{}|{}", p.id, p.name, p.ip, p.version))
            .collect();
        if Some(&sig) == self.peers_sig.as_ref() {
            self.peers = peers;
            return;
        }
        // выбор сохраняем, если комп ещё жив; единственный — выбираем сразу
        let alive = |id: &String| peers.iter().any(|p| &p.id == id);
        match &self.selected {
            Some(id) if alive(id) => {}
            _ => self.selected = None,
        }
        if peers.len() == 1 {
            self.selected = Some(peers[0].id.clone());
        }
        let max_scroll = peers.len().saturating_sub(ROWS_VISIBLE);
        self.peer_scroll = self.peer_scroll.min(max_scroll);
        self.peers = peers;
        self.peers_sig = Some(sig);
        self.invalidate();
    }

    fn selected_peer(&self) -> Option<Peer> {
        let id = self.selected.as_ref()?;
        self.peers.iter().find(|p| &p.id == id).cloned()
    }

    // --- клики ---

    fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Gear => self.settings_menu(),
            Hit::Peer(i) => {
                if let Some(p) = self.peers.get(i) {
                    self.selected = Some(p.id.clone());
                    self.invalidate();
                }
            }
            Hit::BtnClip => self.send_clipboard(),
            Hit::BtnFiles => self.send_files(),
            _ => {}
        }
    }

    fn require_peer(&self) -> Option<Peer> {
        let peer = self.selected_peer();
        if peer.is_none() {
            dialogs::info(self.hwnd, &tr("select_peer"));
        }
        peer
    }

    fn send_clipboard(&mut self) {
        let Some(peer) = self.require_peer() else { return };
        let my_name = self.disco.my_name.clone();
        // в буфере текст — шлём текст; иначе картинка (скриншот и т.п.)
        let text = clipboard::get_text().unwrap_or_default();
        if !text.is_empty() {
            std::thread::spawn(move || {
                match transfer::send_clipboard(&peer.ip, peer.port, &text, &my_name) {
                    Ok(()) => events::log(trf("clip_sent", &[("name", &peer.name)])),
                    Err(e) => events::log(trf("clip_send_fail", &[
                        ("name", &peer.name),
                        ("error", &e.to_string()),
                    ])),
                }
            });
            return;
        }
        let Some(dib) = clipboard::get_dib() else {
            dialogs::info(self.hwnd, &tr("clipboard_empty"));
            return;
        };
        if dib.len() as u64 > config::MAX_IMAGE_SIZE {
            dialogs::info(self.hwnd, &tr("clipboard_empty"));
            return;
        }
        std::thread::spawn(move || {
            match transfer::send_image(&peer.ip, peer.port, &dib, &my_name) {
                Ok(()) => events::log(trf("img_sent", &[("name", &peer.name)])),
                Err(e) => events::log(trf("clip_send_fail", &[
                    ("name", &peer.name),
                    ("error", &e.to_string()),
                ])),
            }
        });
    }

    fn send_files(&mut self) {
        let Some(peer) = self.require_peer() else { return };
        let paths = dialogs::pick_files(self.hwnd, &tr("file_dialog_title"));
        if paths.is_empty() {
            return;
        }
        let my_name = self.disco.my_name.clone();
        std::thread::spawn(move || {
            events::log(trf("sending_files", &[
                ("count", &paths.len().to_string()),
                ("name", &peer.name),
            ]));
            let result = transfer::send_files(&peer.ip, peer.port, &paths, &my_name, |name, size| {
                events::log(trf("file_sent", &[
                    ("file", name),
                    ("size", &transfer::fmt_size(size)),
                ]));
            });
            match result {
                Ok(()) => events::log(trf("files_done", &[("name", &peer.name)])),
                Err(e) => events::log(trf("files_fail", &[
                    ("name", &peer.name),
                    ("error", &e.to_string()),
                ])),
            }
        });
    }

    // --- меню ---

    fn settings_menu(&mut self) {
        let l = self.layout();
        let mut pt = POINT {
            x: (l.gear.l * self.scale) as i32,
            y: (l.gear.b * self.scale) as i32 + 4,
        };
        unsafe {
            let _ = ClientToScreen(self.hwnd, &mut pt);
        }
        let cmd = unsafe { self.build_settings_menu(pt) };
        self.on_menu_command(cmd);
    }

    unsafe fn build_settings_menu(&self, pt: POINT) -> u32 {
        let menu = CreatePopupMenu().unwrap();
        let append = |m: HMENU, flags: MENU_ITEM_FLAGS, id: u32, text: &str| {
            let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
            let _ = AppendMenuW(m, flags, id as usize, PCWSTR(wide.as_ptr()));
        };
        append(
            menu,
            MF_STRING | MF_GRAYED,
            0,
            &format!("{} v{}", config::APP_NAME, config::VERSION),
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        append(menu, MF_STRING, CMD_THEME_AUTO, &tr("theme_auto"));
        append(menu, MF_STRING, CMD_THEME_LIGHT, &tr("theme_light"));
        append(menu, MF_STRING, CMD_THEME_DARK, &tr("theme_dark"));
        let theme_cmd = match self.settings.theme.as_str() {
            "light" => CMD_THEME_LIGHT,
            "dark" => CMD_THEME_DARK,
            _ => CMD_THEME_AUTO,
        };
        let _ = CheckMenuRadioItem(menu, CMD_THEME_AUTO, CMD_THEME_DARK, theme_cmd, MF_BYCOMMAND.0);

        let lang_menu = CreatePopupMenu().unwrap();
        append(lang_menu, MF_STRING, CMD_LANG_AUTO, &tr("lang_auto"));
        for (i, (_, name)) in i18n::LANGUAGES.iter().enumerate() {
            append(lang_menu, MF_STRING, CMD_LANG_AUTO + 1 + i as u32, name);
        }
        let lang_cmd = i18n::LANGUAGES
            .iter()
            .position(|(c, _)| *c == self.settings.lang)
            .map(|i| CMD_LANG_AUTO + 1 + i as u32)
            .unwrap_or(CMD_LANG_AUTO);
        let _ = CheckMenuRadioItem(
            lang_menu,
            CMD_LANG_AUTO,
            CMD_LANG_AUTO + i18n::LANGUAGES.len() as u32,
            lang_cmd,
            MF_BYCOMMAND.0,
        );
        let lang_title: Vec<u16> = tr("menu_language").encode_utf16().chain([0]).collect();
        let _ = AppendMenuW(menu, MF_POPUP, lang_menu.0 as usize, PCWSTR(lang_title.as_ptr()));

        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let auto_flags = if crate::autostart::enabled() {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        append(menu, auto_flags, CMD_AUTOSTART, &tr("menu_autostart"));
        append(menu, MF_STRING, CMD_ROLLOUT, &tr("menu_rollout"));
        append(menu, MF_STRING, CMD_OPEN_FOLDER, &tr("menu_open_folder"));
        append(menu, MF_STRING, CMD_CHANGE_FOLDER, &tr("menu_change_folder"));

        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_LEFTALIGN | TPM_TOPALIGN,
            pt.x,
            pt.y,
            0,
            self.hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        cmd.0 as u32
    }

    fn on_menu_command(&mut self, cmd: u32) {
        match cmd {
            CMD_THEME_AUTO | CMD_THEME_LIGHT | CMD_THEME_DARK => {
                self.settings.theme = match cmd {
                    CMD_THEME_LIGHT => "light",
                    CMD_THEME_DARK => "dark",
                    _ => "auto",
                }
                .into();
                config::save_settings(&self.settings);
                self.dark = resolve_dark(&self.settings.theme);
                self.apply_titlebar();
                self.invalidate();
            }
            CMD_LANG_AUTO => self.set_language("auto".into()),
            c if c > CMD_LANG_AUTO && c <= CMD_LANG_AUTO + i18n::LANGUAGES.len() as u32 => {
                let code = i18n::LANGUAGES[(c - CMD_LANG_AUTO - 1) as usize].0;
                self.set_language(code.into());
            }
            CMD_AUTOSTART => {
                crate::autostart::set(!crate::autostart::enabled());
            }
            CMD_ROLLOUT => self.rollout_update(),
            CMD_OPEN_FOLDER => {
                let dir = config::downloads_dir();
                let _ = std::fs::create_dir_all(&dir);
                let wide: Vec<u16> =
                    dir.display().to_string().encode_utf16().chain([0]).collect();
                unsafe {
                    ShellExecuteW(
                        self.hwnd,
                        w!("open"),
                        PCWSTR(wide.as_ptr()),
                        PCWSTR::null(),
                        PCWSTR::null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
            CMD_CHANGE_FOLDER => {
                if let Some(dir) = dialogs::pick_folder(self.hwnd, &tr("folder_dialog_title")) {
                    config::set_downloads_dir(dir.clone());
                    self.settings.download_dir = dir.display().to_string();
                    config::save_settings(&self.settings);
                    self.push_log(trf("log_downloads", &[(
                        "dir",
                        &dir.display().to_string(),
                    )]));
                }
            }
            _ => {}
        }
    }

    fn set_language(&mut self, code: String) {
        self.settings.lang = code;
        config::save_settings(&self.settings);
        apply_language(&self.settings.lang);
        self.invalidate();
    }

    fn tray_menu(&mut self) {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.hwnd);
            let menu = CreatePopupMenu().unwrap();
            let show: Vec<u16> = tr("tray_show").encode_utf16().chain([0]).collect();
            let exit: Vec<u16> = tr("tray_exit").encode_utf16().chain([0]).collect();
            let _ = AppendMenuW(menu, MF_STRING, CMD_TRAY_SHOW as usize, PCWSTR(show.as_ptr()));
            let _ = SetMenuDefaultItem(menu, CMD_TRAY_SHOW, 0);
            let _ = AppendMenuW(menu, MF_STRING, CMD_TRAY_EXIT as usize, PCWSTR(exit.as_ptr()));
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTALIGN | TPM_BOTTOMALIGN,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(menu);
            match cmd.0 as u32 {
                CMD_TRAY_SHOW => self.show_window(),
                CMD_TRAY_EXIT => self.quit(),
                _ => {}
            }
        }
    }

    fn show_window(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    fn quit(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }

    // --- обновление по сети ---

    fn rollout_update(&mut self) {
        let mine = config::version_tuple(config::VERSION);
        let targets: Vec<Peer> = self
            .disco
            .get_peers()
            .into_iter()
            .filter(|p| config::version_tuple(&p.version) < mine)
            .collect();
        if targets.is_empty() {
            dialogs::info(
                self.hwnd,
                &trf("rollout_all_current", &[("version", config::VERSION)]),
            );
            return;
        }
        let names: Vec<&str> = targets.iter().map(|p| p.name.as_str()).collect();
        let ok = dialogs::confirm(
            self.hwnd,
            &trf("rollout_confirm", &[
                ("version", config::VERSION),
                ("names", &names.join(", ")),
            ]),
        );
        if !ok {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let my_name = self.disco.my_name.clone();
        std::thread::spawn(move || {
            for p in targets {
                events::log(trf("rollout_sending", &[("name", &p.name)]));
                match transfer::send_update(&p.ip, p.port, &exe, config::VERSION, &my_name) {
                    Ok(()) => events::log(trf("rollout_ok", &[
                        ("name", &p.name),
                        ("version", config::VERSION),
                    ])),
                    Err(e) => events::log(trf("rollout_fail", &[
                        ("name", &p.name),
                        ("error", &e.to_string()),
                    ])),
                }
            }
        });
    }

    fn on_update_received(&mut self, path: PathBuf, version: String, sender: String) {
        if config::version_tuple(&version) <= config::version_tuple(config::VERSION) {
            self.push_log(trf("update_skip_old", &[
                ("version", &version),
                ("name", &sender),
                ("current", config::VERSION),
            ]));
            let _ = std::fs::remove_file(&path);
            return;
        }
        self.push_log(trf("update_received", &[
            ("version", &version),
            ("name", &sender),
        ]));
        self.notify(
            &trf("notify_update", &[("version", &version), ("name", &sender)]),
            NotifyAction::ShowWindow,
        );
        self.apply_update(&path);
    }

    /// Заменить свой exe и перезапуститься: работающий exe нельзя перезаписать,
    /// поэтому это делает bat-скрипт после нашего выхода. Пути передаются через
    /// переменные окружения — bat остаётся чистым ASCII при любой кодировке.
    fn apply_update(&mut self, new_exe: &Path) {
        let Ok(target) = std::env::current_exe() else { return };
        let bat = std::env::temp_dir().join("bufernet_update.bat");
        let script = "@echo off\r\n\
:retry\r\n\
ping -n 2 127.0.0.1 >nul\r\n\
copy /y \"%BUFERNET_SRC%\" \"%BUFERNET_DST%\" >nul 2>&1\r\n\
if errorlevel 1 goto retry\r\n\
start \"\" \"%BUFERNET_DST%\"\r\n\
del \"%BUFERNET_SRC%\"\r\n\
del \"%~f0\"\r\n";
        if std::fs::write(&bat, script).is_err() {
            return;
        }
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let spawned = std::process::Command::new("cmd")
            .args(["/c", &bat.display().to_string()])
            .env("BUFERNET_SRC", new_exe)
            .env("BUFERNET_DST", &target)
            .current_dir(target.parent().unwrap_or(Path::new(".")))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        if spawned.is_ok() {
            self.quit();
        }
    }

    // --- отрисовка ---

    fn draw(&mut self) {
        if self.render.is_none() {
            self.render = render::Ctx::new(self.hwnd).ok();
        }
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let (px_w, px_h) = ((rc.right - rc.left) as u32, (rc.bottom - rc.top) as u32);
        if px_w == 0 || px_h == 0 {
            return;
        }
        // ctx забирается из self на время кадра — так поля self остаются доступны
        let Some(mut ctx) = self.render.take() else { return };
        if ctx.ensure_rt(px_w, px_h, self.scale * 96.0).is_err() {
            self.render = Some(ctx);
            return;
        }

        let theme = render::theme(self.dark);
        let l = self.layout();
        let (w, _h) = self.client_size();
        let pad = 16.0;

        ctx.begin(theme.bg);

        // шапка
        ctx.text("BuferNet", Rect::new(pad, 10.0, w, 48.0), 26.0, 600, theme.text, HAlign::Left, false);
        let title_w = ctx.measure_width("BuferNet", 26.0, 600);
        ctx.text(
            &format!("v{}", config::VERSION),
            Rect::new(pad + title_w + 8.0, 24.0, w, 44.0),
            12.5,
            400,
            theme.muted,
            HAlign::Left,
            false,
        );
        ctx.text(
            &format!("💻 {}", self.disco.my_name),
            Rect::new(pad, l.gear.t, l.gear.l - 10.0, l.gear.b),
            12.5,
            400,
            theme.muted,
            HAlign::Right,
            true,
        );
        // шестерёнка
        if matches!(self.hover, Hit::Gear) {
            let c = if self.pressed == Some(Hit::Gear) { theme.press } else { theme.hover };
            ctx.fill_round(l.gear, 6.0, c);
        }
        ctx.text("⚙", l.gear, 16.0, 400, theme.text, HAlign::Center, true);

        // заголовок списка
        ctx.text(
            &tr("peers"),
            Rect::new(pad, 58.0, w - pad, 80.0),
            14.0,
            600,
            theme.text,
            HAlign::Left,
            false,
        );

        // карточка списка
        ctx.fill_round(l.list, 8.0, theme.card);
        ctx.stroke_round(l.list, 8.0, theme.border, 1.0);
        ctx.push_clip(Rect::new(l.list.l, l.list.t + 1.0, l.list.r, l.list.b - 1.0));
        if self.peers.is_empty() {
            ctx.text(
                &tr("searching"),
                Rect::new(l.list.l + 16.0, l.list.t, l.list.r - 16.0, l.list.t + ROW_H + 8.0),
                13.5,
                400,
                theme.muted,
                HAlign::Left,
                true,
            );
        }
        for (i, peer) in self.peers.iter().enumerate() {
            let vi = i as isize - self.peer_scroll as isize;
            if vi < 0 || vi >= ROWS_VISIBLE as isize {
                continue;
            }
            let top = l.list.t + 4.0 + vi as f32 * ROW_H;
            let row = Rect::new(l.list.l + 4.0, top, l.list.r - 4.0, top + ROW_H - 2.0);
            let selected = self.selected.as_deref() == Some(peer.id.as_str());
            if selected {
                ctx.fill_round(row, 6.0, theme.sel_bg);
                // акцентная полоска слева, как в списках Windows 11
                let pill = Rect::new(
                    row.l + 2.0,
                    top + (ROW_H - 2.0) / 2.0 - 8.0,
                    row.l + 5.0,
                    top + (ROW_H - 2.0) / 2.0 + 8.0,
                );
                ctx.fill_round(pill, 1.5, theme.accent);
            } else if self.hover == Hit::Peer(i) {
                ctx.fill_round(row, 6.0, theme.hover);
            }
            ctx.text(
                &format!("💻  {}", peer.name),
                Rect::new(row.l + 14.0, row.t, row.r - 150.0, row.b),
                13.5,
                400,
                theme.text,
                HAlign::Left,
                true,
            );
            let ver = if peer.version.is_empty() {
                tr("old_version")
            } else {
                format!("v{}", peer.version)
            };
            ctx.text(
                &format!("{}  ·  {}", peer.ip, ver),
                Rect::new(row.r - 220.0, row.t, row.r - 12.0, row.b),
                12.0,
                400,
                theme.muted,
                HAlign::Right,
                true,
            );
        }
        ctx.pop_clip();

        // кнопки
        let accent_bg = if self.pressed == Some(Hit::BtnClip) {
            theme.accent_press
        } else if self.hover == Hit::BtnClip {
            theme.accent_hover
        } else {
            theme.accent
        };
        ctx.fill_round(l.btn_clip, 6.0, accent_bg);
        ctx.text(
            &format!("📋  {}", tr("btn_clipboard")),
            l.btn_clip,
            13.5,
            600,
            theme.accent_text,
            HAlign::Center,
            true,
        );
        let files_bg = if self.pressed == Some(Hit::BtnFiles) {
            theme.press
        } else if self.hover == Hit::BtnFiles {
            theme.hover
        } else {
            theme.card
        };
        ctx.fill_round(l.btn_files, 6.0, files_bg);
        ctx.stroke_round(l.btn_files, 6.0, theme.border, 1.0);
        ctx.text(
            &format!("📁  {}", tr("btn_files")),
            l.btn_files,
            13.5,
            400,
            theme.text,
            HAlign::Center,
            true,
        );

        // журнал
        ctx.text(
            &tr("log"),
            Rect::new(pad, l.log.t - 30.0, w - pad, l.log.t - 4.0),
            14.0,
            600,
            theme.text,
            HAlign::Left,
            false,
        );
        ctx.fill_round(l.log, 8.0, theme.log_bg);
        ctx.stroke_round(l.log, 8.0, theme.border, 1.0);
        let content = self.log.join("\n");
        let inner_w = (l.log.w() - 24.0).max(10.0);
        let view_h = l.log.h() - 16.0;
        if let Ok(layout) = ctx.layout(&content, 12.5, 400, inner_w, true) {
            let content_h = render::Ctx::layout_height(&layout);
            let max_scroll = (content_h - view_h).max(0.0);
            if self.log_at_bottom {
                self.log_scroll = max_scroll;
            }
            self.log_scroll = self.log_scroll.clamp(0.0, max_scroll);
            self.log_view = (content_h, view_h);
            ctx.push_clip(Rect::new(l.log.l + 1.0, l.log.t + 1.0, l.log.r - 1.0, l.log.b - 1.0));
            ctx.draw_layout(&layout, l.log.l + 12.0, l.log.t + 8.0 - self.log_scroll, theme.text);
            ctx.pop_clip();
            // ползунок
            if content_h > view_h {
                let track_top = l.log.t + 4.0;
                let track_h = l.log.h() - 8.0;
                let bar_h = (track_h * view_h / content_h).max(24.0);
                let ratio = if max_scroll > 0.0 { self.log_scroll / max_scroll } else { 0.0 };
                let bar_top = track_top + ratio * (track_h - bar_h);
                let wide = matches!(self.hover, Hit::LogBar) || self.drag_bar.is_some();
                let bar_w = if wide { 6.0 } else { 3.0 };
                let bar = Rect::new(l.log.r - 6.0 - bar_w, bar_top, l.log.r - 6.0, bar_top + bar_h);
                ctx.fill_round(bar, bar_w / 2.0, theme.scrollbar);
                self.log_bar = Some(bar);
            } else {
                self.log_bar = None;
            }
        }

        ctx.end();
        self.render = Some(ctx);
    }
}
