"""Обнаружение других компьютеров в локальной сети через UDP-broadcast.

Каждый экземпляр программы раз в ANNOUNCE_INTERVAL секунд шлёт в сеть
JSON-пакет {"app": "bufernet", "id": ..., "name": ..., "port": ...}
и одновременно слушает такие же пакеты от других.
"""
import json
import socket
import threading
import time
import uuid
from dataclasses import dataclass, field

from . import config


@dataclass
class Peer:
    peer_id: str
    name: str
    ip: str
    port: int
    version: str = ""  # пустая — версия старше 1.1, ещё не умела представляться
    last_seen: float = field(default_factory=time.monotonic)

    @property
    def alive(self) -> bool:
        return time.monotonic() - self.last_seen < config.PEER_TIMEOUT


class Discovery:
    def __init__(self, transfer_port: int):
        self.my_id = uuid.uuid4().hex
        self.my_name = socket.gethostname()
        self.transfer_port = transfer_port
        self._peers: dict[str, Peer] = {}
        self._lock = threading.Lock()
        self._stop = threading.Event()

    def start(self):
        threading.Thread(target=self._announce_loop, daemon=True).start()
        threading.Thread(target=self._listen_loop, daemon=True).start()

    def stop(self):
        self._stop.set()

    def get_peers(self) -> list[Peer]:
        with self._lock:
            return sorted(
                (p for p in self._peers.values() if p.alive),
                key=lambda p: p.name.lower(),
            )

    # --- внутреннее ---

    def _announce_loop(self):
        payload = json.dumps({
            "app": "bufernet",
            "id": self.my_id,
            "name": self.my_name,
            "port": self.transfer_port,
            "version": config.VERSION,
        }).encode("utf-8")
        while not self._stop.is_set():
            # Broadcast уходит только через один интерфейс, поэтому шлём с каждого
            # локального IP отдельно (Wi-Fi, Ethernet, VPN и т.д.) + loopback,
            # чтобы находились и копии программы на этой же машине.
            for src_ip in self._local_ips():
                sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                sock.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
                try:
                    if src_ip:
                        sock.bind((src_ip, 0))
                    dest = "127.255.255.255" if src_ip == "127.0.0.1" else "255.255.255.255"
                    sock.sendto(payload, (dest, config.DISCOVERY_PORT))
                except OSError:
                    pass  # интерфейс мог пропасть — попробуем в следующий раз
                finally:
                    sock.close()
            self._stop.wait(config.ANNOUNCE_INTERVAL)

    @staticmethod
    def _local_ips() -> set[str]:
        ips = {"", "127.0.0.1"}  # "" — отправка через интерфейс по умолчанию
        try:
            infos = socket.getaddrinfo(socket.gethostname(), None, socket.AF_INET)
            ips.update(info[4][0] for info in infos)
        except OSError:
            pass
        return ips

    def _listen_loop(self):
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        sock.settimeout(1.0)
        sock.bind(("", config.DISCOVERY_PORT))
        while not self._stop.is_set():
            try:
                data, addr = sock.recvfrom(4096)
            except socket.timeout:
                continue
            except OSError:
                break
            try:
                msg = json.loads(data.decode("utf-8"))
            except (ValueError, UnicodeDecodeError):
                continue
            if msg.get("app") != "bufernet" or msg.get("id") == self.my_id:
                continue
            peer = Peer(
                peer_id=msg["id"],
                name=str(msg.get("name", addr[0])),
                ip=addr[0],
                port=int(msg["port"]),
                version=str(msg.get("version", "")),
            )
            with self._lock:
                self._peers[peer.peer_id] = peer
        sock.close()
