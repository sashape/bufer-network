"""Общие настройки приложения."""
import json
from pathlib import Path

APP_NAME = "BuferNet"
VERSION = "1.1.0"


def version_tuple(v: str) -> tuple:
    """'1.2.10' -> (1, 2, 10) для сравнения версий; мусор считается самой старой."""
    try:
        return tuple(int(x) for x in v.split("."))
    except (ValueError, AttributeError):
        return (0,)


# UDP-порт, на котором компьютеры объявляют о себе в локальной сети
DISCOVERY_PORT = 48765
# TCP-порт для приёма буфера и файлов (если занят — возьмётся свободный)
TRANSFER_PORT = 48766

ANNOUNCE_INTERVAL = 3.0   # как часто объявлять о себе, сек
PEER_TIMEOUT = 10.0       # через сколько секунд молчания комп считается пропавшим

# Куда сохранять принятые файлы
DOWNLOADS_DIR = Path.home() / "Downloads" / "BuferNet"

# Максимальный размер текста буфера обмена (защита от мусора), байт
MAX_CLIPBOARD_SIZE = 16 * 1024 * 1024

# Пользовательские настройки (тема и т.п.)
SETTINGS_FILE = Path.home() / ".bufernet.json"


def load_settings() -> dict:
    try:
        return json.loads(SETTINGS_FILE.read_text("utf-8"))
    except (OSError, ValueError):
        return {}


def save_settings(settings: dict):
    try:
        SETTINGS_FILE.write_text(
            json.dumps(settings, ensure_ascii=False, indent=2), "utf-8"
        )
    except OSError:
        pass  # настройки не критичны
