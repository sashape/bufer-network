//! Очередь событий из фоновых потоков в поток GUI.
//! GUI выгребает её по таймеру каждые 200 мс (как _ui_queue в Python-версии).

use std::path::PathBuf;
use std::sync::Mutex;

pub enum UiEvent {
    Log(String),
    ClipboardReceived { text: String, sender: String },
    FileReceived { path: PathBuf, sender: String },
    UpdateReceived { path: PathBuf, version: String, sender: String },
}

static EVENTS: Mutex<Vec<UiEvent>> = Mutex::new(Vec::new());

pub fn push(ev: UiEvent) {
    EVENTS.lock().unwrap().push(ev);
}

pub fn log(msg: String) {
    push(UiEvent::Log(msg));
}

pub fn drain() -> Vec<UiEvent> {
    std::mem::take(&mut *EVENTS.lock().unwrap())
}
