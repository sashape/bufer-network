//! Приём и отправка буфера обмена и файлов по TCP
//! (порт bufernet/transfer.py, протокол совместим с Python-версией).
//!
//! Протокол: по одному TCP-соединению идёт последовательность элементов.
//! Каждый элемент — строка JSON, завершённая \n, затем ровно `size` байт данных:
//!
//!     {"type": "hello", "name": "PC-1"}\n
//!     {"type": "clipboard", "size": 12}\n<12 байт utf-8>
//!     {"type": "file", "name": "photo.jpg", "size": 123456}\n<123456 байт>
//!     {"type": "end"}\n

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config;
use crate::events::{self, UiEvent};
use crate::i18n::{tr, trf};
use crate::json::{self, JVal};

const CHUNK: usize = 64 * 1024;

/// Слушает TCP-порт и принимает входящие буферы/файлы.
/// Результаты уходят в очередь событий GUI.
pub struct Server {
    pub port: u16,
    listener: TcpListener,
}

impl Server {
    pub fn bind() -> io::Result<Server> {
        // порт занят (например, вторая копия программы) — берём любой свободный
        let listener = TcpListener::bind(("0.0.0.0", config::TRANSFER_PORT))
            .or_else(|_| TcpListener::bind(("0.0.0.0", 0)))?;
        let port = listener.local_addr()?.port();
        Ok(Server { port, listener })
    }

    pub fn start(self) {
        std::thread::spawn(move || {
            for conn in self.listener.incoming() {
                let Ok(conn) = conn else { continue };
                let ip = conn
                    .peer_addr()
                    .map(|a| a.ip().to_string())
                    .unwrap_or_default();
                std::thread::spawn(move || {
                    let mut sender = ip.clone();
                    if let Err(e) = handle(conn, &mut sender) {
                        // приём не должен ронять программу
                        events::log(trf(
                            "recv_error",
                            &[("name", &sender), ("error", &e.to_string())],
                        ));
                    }
                });
            }
        });
    }
}

fn handle(conn: TcpStream, sender: &mut String) -> io::Result<()> {
    conn.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut writer = conn.try_clone()?; // для подтверждения приёма обновления
    let mut reader = BufReader::new(conn);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let header = json::parse_object(line.trim_end())
            .ok_or_else(|| bad_data("malformed header"))?;
        let get_str = |k: &str| header.get(k).and_then(JVal::as_str).map(str::to_owned);
        let get_size = || -> io::Result<u64> {
            header
                .get("size")
                .and_then(JVal::as_int)
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(|| bad_data("bad size"))
        };
        match get_str("type").as_deref() {
            Some("hello") => {
                if let Some(name) = get_str("name") {
                    *sender = name;
                }
            }
            Some("clipboard") => {
                let size = get_size()?;
                if size > config::MAX_CLIPBOARD_SIZE {
                    return Err(bad_data("clipboard too large"));
                }
                let mut data = vec![0u8; size as usize];
                reader.read_exact(&mut data)?;
                let text = String::from_utf8(data).map_err(|_| bad_data("bad utf-8"))?;
                events::push(UiEvent::ClipboardReceived {
                    text,
                    sender: sender.clone(),
                });
            }
            Some("file") => {
                let name = get_str("name").ok_or_else(|| bad_data("no file name"))?;
                let path = receive_file(&mut reader, &name, get_size()?)?;
                events::push(UiEvent::FileReceived {
                    path,
                    sender: sender.clone(),
                });
            }
            Some("update") => {
                let version = get_str("version").unwrap_or_default();
                let path = receive_update(&mut reader, &version, get_size()?)?;
                writer.write_all(b"OK")?; // подтверждаем приём отправителю
                events::push(UiEvent::UpdateReceived {
                    path,
                    version,
                    sender: sender.clone(),
                });
            }
            Some("end") => return Ok(()),
            other => {
                return Err(bad_data(&format!(
                    "unknown item type: {}",
                    other.unwrap_or("?")
                )))
            }
        }
    }
}

fn receive_file(reader: &mut impl Read, name: &str, size: u64) -> io::Result<PathBuf> {
    // берём только имя файла, отбрасывая любые пути от отправителя
    let name = Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    let dir = config::downloads_dir();
    std::fs::create_dir_all(&dir)?;
    let path = unique_path(dir.join(name));
    stream_to_file(reader, &path, size)?;
    Ok(path)
}

fn receive_update(reader: &mut impl Read, version: &str, size: u64) -> io::Result<PathBuf> {
    let safe: String = version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    let safe = if safe.is_empty() { "unknown".into() } else { safe };
    let path = std::env::temp_dir().join(format!("bufernet_update_{safe}.exe"));
    stream_to_file(reader, &path, size)?;
    Ok(path)
}

fn stream_to_file(reader: &mut impl Read, path: &Path, size: u64) -> io::Result<()> {
    let result = (|| {
        let mut file = std::fs::File::create(path)?;
        let written = io::copy(&mut reader.take(size), &mut file)?;
        if written < size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                tr("err_conn_lost"),
            ));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(path); // не оставляем недокачанный файл
    }
    result
}

/// photo.jpg -> photo (1).jpg, если файл уже есть.
fn unique_path(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 1.. {
        let candidate = path.with_file_name(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

fn bad_data(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

// --- отправка ---

fn connect(ip: &str, port: u16) -> io::Result<TcpStream> {
    let addr: SocketAddr = format!("{ip}:{port}")
        .parse()
        .map_err(|_| bad_data("bad address"))?;
    TcpStream::connect_timeout(&addr, Duration::from_secs(10))
}

fn send_header(sock: &mut TcpStream, pairs: &[(&str, String)]) -> io::Result<()> {
    sock.write_all(format!("{}\n", json::object(pairs)).as_bytes())
}

fn hello(sock: &mut TcpStream, my_name: &str) -> io::Result<()> {
    send_header(sock, &[
        ("type", json::quote("hello")),
        ("name", json::quote(my_name)),
    ])
}

fn end(sock: &mut TcpStream) -> io::Result<()> {
    send_header(sock, &[("type", json::quote("end"))])
}

pub fn send_clipboard(ip: &str, port: u16, text: &str, my_name: &str) -> io::Result<()> {
    let data = text.as_bytes();
    let mut sock = connect(ip, port)?;
    hello(&mut sock, my_name)?;
    send_header(&mut sock, &[
        ("type", json::quote("clipboard")),
        ("size", data.len().to_string()),
    ])?;
    sock.write_all(data)?;
    end(&mut sock)
}

pub fn send_files(
    ip: &str,
    port: u16,
    paths: &[PathBuf],
    my_name: &str,
    mut on_progress: impl FnMut(&str, u64),
) -> io::Result<()> {
    let mut sock = connect(ip, port)?;
    sock.set_write_timeout(Some(Duration::from_secs(60)))?;
    hello(&mut sock, my_name)?;
    for path in paths {
        let size = path.metadata()?.len();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        send_header(&mut sock, &[
            ("type", json::quote("file")),
            ("name", json::quote(&name)),
            ("size", size.to_string()),
        ])?;
        copy_file(path, &mut sock)?;
        on_progress(&name, size);
    }
    end(&mut sock)
}

/// Отправить свой exe как обновление. Ждём подтверждение "OK" —
/// старые версии BuferNet его не шлют, и мы честно сообщим об ошибке.
pub fn send_update(ip: &str, port: u16, exe_path: &Path, version: &str, my_name: &str) -> io::Result<()> {
    let size = exe_path.metadata()?.len();
    let mut sock = connect(ip, port)?;
    sock.set_write_timeout(Some(Duration::from_secs(120)))?;
    sock.set_read_timeout(Some(Duration::from_secs(120)))?;
    hello(&mut sock, my_name)?;
    send_header(&mut sock, &[
        ("type", json::quote("update")),
        ("version", json::quote(version)),
        ("size", size.to_string()),
    ])?;
    copy_file(exe_path, &mut sock)?;
    end(&mut sock)?;
    let mut ack = [0u8; 2];
    match sock.read_exact(&mut ack) {
        Ok(()) if &ack == b"OK" => Ok(()),
        _ => Err(io::Error::other(tr("err_no_ack"))),
    }
}

fn copy_file(path: &Path, sock: &mut TcpStream) -> io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        sock.write_all(&buf[..n])?;
    }
}

pub fn fmt_size(size: u64) -> String {
    let mut size = size as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if size < 1024.0 || unit == "GB" {
            return if unit == "B" {
                format!("{size:.0} {unit}")
            } else {
                format!("{size:.1} {unit}")
            };
        }
        size /= 1024.0;
    }
    unreachable!()
}
