"""Приём и отправка буфера обмена и файлов по TCP.

Протокол: по одному TCP-соединению идёт последовательность элементов.
Каждый элемент — строка JSON, завершённая \n, затем ровно `size` байт данных:

    {"type": "hello", "name": "PC-1"}\n
    {"type": "clipboard", "size": 12}\n<12 байт utf-8>
    {"type": "file", "name": "photo.jpg", "size": 123456}\n<123456 байт>
    {"type": "end"}\n
"""
import json
import socket
import tempfile
import threading
from pathlib import Path
from typing import Callable

from . import config
from .i18n import tr

CHUNK = 64 * 1024


class TransferServer:
    """Слушает TCP-порт и принимает входящие буферы/файлы."""

    def __init__(
        self,
        on_clipboard: Callable[[str, str], None],        # (text, sender_name)
        on_file: Callable[[Path, str], None],            # (saved_path, sender_name)
        on_error: Callable[[str], None],
        on_update: Callable[[Path, str, str], None] = lambda p, v, s: None,
        # on_update: (saved_exe_path, version, sender_name)
    ):
        self.on_clipboard = on_clipboard
        self.on_file = on_file
        self.on_error = on_error
        self.on_update = on_update
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        # эксклюзивная привязка: иначе на Windows вторая копия программы может
        # «сесть» на тот же порт и перехватывать чужие соединения
        try:
            self._sock.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        except (AttributeError, OSError):
            pass
        try:
            self._sock.bind(("", config.TRANSFER_PORT))
        except OSError:
            # порт занят (например, вторая копия программы) — берём любой свободный
            self._sock.bind(("", 0))
        self.port = self._sock.getsockname()[1]
        self._stop = threading.Event()

    def start(self):
        self._sock.listen(5)
        threading.Thread(target=self._accept_loop, daemon=True).start()

    def stop(self):
        self._stop.set()
        try:
            self._sock.close()
        except OSError:
            pass

    def _accept_loop(self):
        while not self._stop.is_set():
            try:
                conn, addr = self._sock.accept()
            except OSError:
                break
            threading.Thread(
                target=self._handle, args=(conn, addr[0]), daemon=True
            ).start()

    def _handle(self, conn: socket.socket, ip: str):
        sender = ip
        try:
            with conn:
                conn.settimeout(30)
                reader = conn.makefile("rb")
                while True:
                    line = reader.readline()
                    if not line:
                        break
                    header = json.loads(line.decode("utf-8"))
                    kind = header.get("type")
                    if kind == "hello":
                        sender = str(header.get("name", ip))
                    elif kind == "clipboard":
                        size = int(header["size"])
                        if size > config.MAX_CLIPBOARD_SIZE:
                            raise ValueError("clipboard too large")
                        text = self._read_exact(reader, size).decode("utf-8")
                        self.on_clipboard(text, sender)
                    elif kind == "file":
                        path = self._receive_file(reader, header)
                        self.on_file(path, sender)
                    elif kind == "update":
                        path = self._receive_update(reader, header)
                        conn.sendall(b"OK")  # подтверждаем приём отправителю
                        self.on_update(path, str(header.get("version", "")), sender)
                    elif kind == "end":
                        break
                    else:
                        raise ValueError(f"unknown item type: {kind}")
        except Exception as e:  # приём не должен ронять программу
            self.on_error(tr("recv_error", name=sender, error=e))

    def _receive_file(self, reader, header: dict) -> Path:
        # берём только имя файла, отбрасывая любые пути от отправителя
        name = Path(str(header["name"])).name or "file"
        config.DOWNLOADS_DIR.mkdir(parents=True, exist_ok=True)
        path = _unique_path(config.DOWNLOADS_DIR / name)
        self._stream_to_file(reader, path, int(header["size"]))
        return path

    def _receive_update(self, reader, header: dict) -> Path:
        version = str(header.get("version", ""))
        safe = "".join(c for c in version if c.isalnum() or c == ".") or "unknown"
        path = Path(tempfile.gettempdir()) / f"bufernet_update_{safe}.exe"
        self._stream_to_file(reader, path, int(header["size"]))
        return path

    @staticmethod
    def _stream_to_file(reader, path: Path, size: int):
        try:
            with open(path, "wb") as f:
                remaining = size
                while remaining > 0:
                    chunk = reader.read(min(CHUNK, remaining))
                    if not chunk:
                        raise ConnectionError(tr("err_conn_lost"))
                    f.write(chunk)
                    remaining -= len(chunk)
        except Exception:
            path.unlink(missing_ok=True)  # не оставляем недокачанный файл
            raise

    @staticmethod
    def _read_exact(reader, size: int) -> bytes:
        data = b""
        while len(data) < size:
            chunk = reader.read(size - len(data))
            if not chunk:
                raise ConnectionError(tr("err_conn_lost"))
            data += chunk
        return data


def _unique_path(path: Path) -> Path:
    """photo.jpg -> photo (1).jpg, если файл уже есть."""
    if not path.exists():
        return path
    n = 1
    while True:
        candidate = path.with_name(f"{path.stem} ({n}){path.suffix}")
        if not candidate.exists():
            return candidate
        n += 1


# --- отправка ---

def _send_header(sock: socket.socket, **fields):
    sock.sendall(json.dumps(fields).encode("utf-8") + b"\n")


def send_clipboard(ip: str, port: int, text: str, my_name: str):
    data = text.encode("utf-8")
    with socket.create_connection((ip, port), timeout=10) as sock:
        _send_header(sock, type="hello", name=my_name)
        _send_header(sock, type="clipboard", size=len(data))
        sock.sendall(data)
        _send_header(sock, type="end")


def send_files(
    ip: str,
    port: int,
    paths: list[Path],
    my_name: str,
    on_progress: Callable[[str, int], None] = lambda name, size: None,
):
    with socket.create_connection((ip, port), timeout=10) as sock:
        sock.settimeout(60)
        _send_header(sock, type="hello", name=my_name)
        for path in paths:
            size = path.stat().st_size
            _send_header(sock, type="file", name=path.name, size=size)
            with open(path, "rb") as f:
                while chunk := f.read(CHUNK):
                    sock.sendall(chunk)
            on_progress(path.name, size)
        _send_header(sock, type="end")


def send_update(ip: str, port: int, exe_path: Path, version: str, my_name: str):
    """Отправить свой exe как обновление. Ждём подтверждение "OK" —
    старые версии BuferNet его не шлют, и мы честно сообщим об ошибке."""
    size = exe_path.stat().st_size
    with socket.create_connection((ip, port), timeout=10) as sock:
        sock.settimeout(120)
        _send_header(sock, type="hello", name=my_name)
        _send_header(sock, type="update", version=version, size=size)
        with open(exe_path, "rb") as f:
            while chunk := f.read(CHUNK):
                sock.sendall(chunk)
        _send_header(sock, type="end")
        if sock.recv(2) != b"OK":
            raise RuntimeError(tr("err_no_ack"))


def fmt_size(size: int) -> str:
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024 or unit == "GB":
            return f"{size:.0f} {unit}" if unit == "B" else f"{size:.1f} {unit}"
        size /= 1024
