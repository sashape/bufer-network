//! Обнаружение других компьютеров в локальной сети через UDP-broadcast
//! (порт bufernet/discovery.py, протокол совместим с Python-версией).
//!
//! Каждый экземпляр раз в ANNOUNCE_INTERVAL шлёт в сеть JSON-пакет
//! {"app": "bufernet", "id": ..., "name": ..., "port": ..., "version": ...}
//! и одновременно слушает такие же пакеты от других.

use std::collections::{HashMap, HashSet};
use std::net::{ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config;
use crate::json::{self, JVal};

#[derive(Clone)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub version: String, // пустая — версия старше 1.1, ещё не умела представляться
    pub last_seen: Instant,
}

impl Peer {
    pub fn alive(&self) -> bool {
        self.last_seen.elapsed() < config::PEER_TIMEOUT
    }
}

pub struct Discovery {
    pub my_id: String,
    pub my_name: String,
    transfer_port: u16,
    peers: Mutex<HashMap<String, Peer>>,
}

impl Discovery {
    pub fn new(transfer_port: u16) -> Arc<Self> {
        Arc::new(Discovery {
            my_id: random_id(),
            my_name: hostname(),
            transfer_port,
            peers: Mutex::new(HashMap::new()),
        })
    }

    pub fn start(self: &Arc<Self>) {
        let a = self.clone();
        std::thread::spawn(move || a.announce_loop());
        let l = self.clone();
        std::thread::spawn(move || l.listen_loop());
    }

    pub fn get_peers(&self) -> Vec<Peer> {
        let mut peers: Vec<Peer> = self
            .peers
            .lock()
            .unwrap()
            .values()
            .filter(|p| p.alive())
            .cloned()
            .collect();
        peers.sort_by_key(|p| p.name.to_lowercase());
        peers
    }

    // --- внутреннее ---

    fn announce_loop(&self) {
        let payload = json::object(&[
            ("app", json::quote("bufernet")),
            ("id", json::quote(&self.my_id)),
            ("name", json::quote(&self.my_name)),
            ("port", self.transfer_port.to_string()),
            ("version", json::quote(config::VERSION)),
        ]);
        let payload = payload.as_bytes();
        loop {
            // Broadcast уходит только через один интерфейс, поэтому шлём с каждого
            // локального IP отдельно (Wi-Fi, Ethernet, VPN и т.д.) + loopback,
            // чтобы находились и копии программы на этой же машине.
            for src_ip in local_ips() {
                let bind_addr = if src_ip.is_empty() { "0.0.0.0" } else { &src_ip };
                let Ok(sock) = UdpSocket::bind((bind_addr, 0)) else {
                    continue; // интерфейс мог пропасть — попробуем в следующий раз
                };
                let _ = sock.set_broadcast(true);
                let dest = if src_ip == "127.0.0.1" {
                    "127.255.255.255"
                } else {
                    "255.255.255.255"
                };
                let _ = sock.send_to(payload, (dest, config::DISCOVERY_PORT));
            }
            std::thread::sleep(config::ANNOUNCE_INTERVAL);
        }
    }

    fn listen_loop(&self) {
        let Some(sock) = bind_listen_socket() else {
            crate::events::log(format!(
                "UDP port {} busy — discovery disabled", config::DISCOVERY_PORT
            ));
            return;
        };
        let _ = sock.set_read_timeout(Some(Duration::from_secs(1)));
        let mut buf = [0u8; 4096];
        loop {
            let (len, addr) = match sock.recv_from(&mut buf) {
                Ok(r) => r,
                Err(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) => continue,
                Err(_) => continue, // ICMP port unreachable и прочий шум
            };
            let Ok(text) = std::str::from_utf8(&buf[..len]) else {
                continue;
            };
            let Some(msg) = json::parse_object(text) else {
                continue;
            };
            let get_str = |k: &str| msg.get(k).and_then(JVal::as_str).map(str::to_owned);
            if get_str("app").as_deref() != Some("bufernet") {
                continue;
            }
            let Some(id) = get_str("id") else { continue };
            if id == self.my_id {
                continue;
            }
            let Some(port) = msg.get("port").and_then(JVal::as_int) else {
                continue;
            };
            let ip = addr.ip().to_string();
            let peer = Peer {
                name: get_str("name").unwrap_or_else(|| ip.clone()),
                ip,
                port: port as u16,
                version: get_str("version").unwrap_or_default(),
                last_seen: Instant::now(),
                id: id.clone(),
            };
            self.peers.lock().unwrap().insert(id, peer);
        }
    }
}

fn hostname() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

/// Все локальные IPv4 (через getaddrinfo по имени хоста) + loopback.
/// Пустая строка — отправка через интерфейс по умолчанию.
fn local_ips() -> HashSet<String> {
    let mut ips: HashSet<String> = HashSet::new();
    ips.insert(String::new());
    ips.insert("127.0.0.1".into());
    if let Ok(addrs) = (hostname().as_str(), 0u16).to_socket_addrs() {
        for a in addrs {
            if a.is_ipv4() {
                ips.insert(a.ip().to_string());
            }
        }
    }
    ips
}

/// UDP-сокет на DISCOVERY_PORT с SO_REUSEADDR, чтобы несколько копий
/// программы на одной машине могли слушать одновременно. std такого не даёт,
/// поэтому сокет создаётся напрямую через Winsock.
fn bind_listen_socket() -> Option<UdpSocket> {
    use std::os::windows::io::FromRawSocket;
    use windows::Win32::Networking::WinSock as ws;

    // std лениво делает WSAStartup при первом использовании сетей —
    // гарантируем это до прямых вызовов Winsock
    let _ = UdpSocket::bind("127.0.0.1:0");

    unsafe {
        let Ok(s) = ws::socket(ws::AF_INET.0 as i32, ws::SOCK_DGRAM, 0) else {
            return None;
        };
        let one: [u8; 4] = 1i32.to_ne_bytes();
        let _ = ws::setsockopt(s, ws::SOL_SOCKET as i32, ws::SO_REUSEADDR as i32, Some(&one));
        let addr = ws::SOCKADDR_IN {
            sin_family: ws::AF_INET,
            sin_port: config::DISCOVERY_PORT.to_be(),
            ..Default::default()
        };
        if ws::bind(s, &addr as *const _ as *const ws::SOCKADDR, size_of::<ws::SOCKADDR_IN>() as i32) != 0 {
            let _ = ws::closesocket(s);
            return None;
        }
        Some(UdpSocket::from_raw_socket(s.0 as _))
    }
}

/// Уникальный id экземпляра: криптостойкость не нужна, важна только
/// уникальность среди соседей по сети.
fn random_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id() as u128;
    let stack_entropy = &nanos as *const _ as u128;
    let mut x = nanos ^ (pid << 64) ^ stack_entropy;
    // xorshift, чтобы биты перемешались
    let mut out = String::with_capacity(32);
    for _ in 0..2 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.push_str(&format!("{:016x}", (x as u64)));
    }
    out
}

use std::mem::size_of;
