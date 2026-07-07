"""Главное окно, иконка в трее и связка всех частей.

Интерфейс в стиле Windows 11: тема Sun Valley (sv-ttk) + эффект Mica
для окна (pywinstyles). Тёмная/светлая тема и язык берутся из настроек
Windows, но переключаются в меню ⚙.
"""
import ctypes
import os
import queue
import subprocess
import sys
import tempfile
import threading
import tkinter as tk
from pathlib import Path
from tkinter import filedialog, messagebox, ttk

import pyperclip
import pystray
import sv_ttk
from PIL import Image, ImageDraw, ImageTk

try:
    import pywinstyles
except ImportError:  # не критично: просто не будет эффекта Mica
    pywinstyles = None

from . import config, discovery, i18n, transfer
from .i18n import tr


def _enable_dpi_awareness():
    """Объявляем процесс DPI-aware, чтобы окно не размывалось при масштабе
    Windows 125/150%. Вызывать до создания tk.Tk()."""
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(1)  # SYSTEM_DPI_AWARE
    except (AttributeError, OSError):
        try:
            ctypes.windll.user32.SetProcessDPIAware()  # запасной путь для старых Windows
        except (AttributeError, OSError):
            pass


def _windows_is_dark() -> bool:
    """Тёмная ли тема приложений в настройках Windows."""
    try:
        import winreg
        key = winreg.OpenKey(
            winreg.HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        )
        value, _ = winreg.QueryValueEx(key, "AppsUseLightTheme")
        return value == 0
    except OSError:
        return False


def _make_icon_image() -> Image.Image:
    """Простая иконка: синий квадрат со стрелками туда-обратно."""
    img = Image.new("RGBA", (64, 64), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([4, 4, 60, 60], radius=14, fill=(0, 103, 192, 255))
    d.line([16, 26, 44, 26], fill="white", width=6)
    d.polygon([(44, 18), (56, 26), (44, 34)], fill="white")
    d.line([48, 42, 20, 42], fill="white", width=6)
    d.polygon([(20, 34), (8, 42), (20, 50)], fill="white")
    return img


class App:
    def __init__(self):
        # сеть
        self.server = transfer.TransferServer(
            on_clipboard=self._on_clipboard_received,
            on_file=self._on_file_received,
            on_error=lambda msg: self._ui(self._log, msg),
            on_update=self._on_update_received,
        )
        self.disco = discovery.Discovery(transfer_port=self.server.port)

        # очередь для вызовов из фоновых потоков в поток GUI
        self._ui_queue: queue.Queue = queue.Queue()

        # настройки: тема, язык, папка сохранения
        self.settings = config.load_settings()
        saved_dir = self.settings.get("download_dir")
        if saved_dir:
            config.DOWNLOADS_DIR = Path(saved_dir)

        # окно
        _enable_dpi_awareness()
        self.root = tk.Tk()
        self.root.title(config.APP_NAME)
        # коэффициент масштаба Windows (1.0 при 100%, 1.25 при 125% и т.д.);
        # шрифты Tk масштабирует сам, а размеры в пикселях — через self._px()
        self.scale = self.root.winfo_fpixels("1i") / 96
        self.root.geometry(f"{self._px(440)}x{self._px(560)}")
        self.root.minsize(self._px(400), self._px(460))
        self.root.protocol("WM_DELETE_WINDOW", self._hide_window)

        # язык: "auto" (как в Windows) или код из i18n.LANGUAGES
        self._lang_var = tk.StringVar(value=self.settings.get("lang", "auto"))
        self._apply_language()

        # тема: "auto" (как в Windows) / "light" / "dark"
        self._theme_var = tk.StringVar(value=self.settings.get("theme", "auto"))
        self.dark = self._resolve_dark()
        sv_ttk.set_theme("dark" if self.dark else "light", self.root)
        self._apply_window_effects()

        self._icon_image = _make_icon_image()
        self.root.iconphoto(True, ImageTk.PhotoImage(self._icon_image))

        self._build_ui()

        # трей
        self.tray = pystray.Icon(
            config.APP_NAME, self._icon_image, config.APP_NAME,
            menu=self._tray_menu(),
        )

    def _px(self, n: int) -> int:
        """Пиксели с учётом масштаба Windows."""
        return round(n * self.scale)

    def _tray_menu(self) -> pystray.Menu:
        return pystray.Menu(
            pystray.MenuItem(
                tr("tray_show"), lambda: self._ui(self._show_window), default=True,
            ),
            pystray.MenuItem(tr("tray_exit"), lambda: self._ui(self._quit)),
        )

    def _resolve_dark(self) -> bool:
        theme = self._theme_var.get()
        if theme == "auto":
            return _windows_is_dark()
        return theme == "dark"

    def _apply_window_effects(self):
        """Mica-подложка и тёмный заголовок окна (Windows 11)."""
        if not pywinstyles:
            return
        try:
            pywinstyles.apply_style(self.root, "mica" if self.dark else "normal")
            pywinstyles.change_header_color(
                self.root, "#1c1c1c" if self.dark else "#fafafa"
            )
        except Exception:
            pass  # на Windows 10 и старше эффектов просто не будет

    # --- настройки: тема, язык, папка ---

    def _show_settings_menu(self):
        menu = tk.Menu(self.root, tearoff=0, font=("Segoe UI", 10))
        menu.add_command(label=f"{config.APP_NAME} v{config.VERSION}", state="disabled")
        menu.add_separator()
        for label_key, value in (
            ("theme_auto", "auto"), ("theme_light", "light"), ("theme_dark", "dark"),
        ):
            menu.add_radiobutton(
                label=tr(label_key), value=value, variable=self._theme_var,
                command=lambda v=value: self._set_theme(v),
            )
        lang_menu = tk.Menu(menu, tearoff=0, font=("Segoe UI", 10))
        lang_menu.add_radiobutton(
            label=tr("lang_auto"), value="auto", variable=self._lang_var,
            command=lambda: self._set_language("auto"),
        )
        for code, name in i18n.LANGUAGES.items():
            lang_menu.add_radiobutton(
                label=name, value=code, variable=self._lang_var,
                command=lambda c=code: self._set_language(c),
            )
        menu.add_cascade(label=tr("menu_language"), menu=lang_menu)
        menu.add_separator()
        menu.add_command(label=tr("menu_rollout"), command=self._rollout_update)
        menu.add_command(label=tr("menu_open_folder"), command=self._open_downloads)
        menu.add_command(label=tr("menu_change_folder"), command=self._change_folder)
        btn = self._settings_btn
        menu.tk_popup(btn.winfo_rootx(), btn.winfo_rooty() + btn.winfo_height())

    def _set_theme(self, value: str):
        self.settings["theme"] = value
        config.save_settings(self.settings)
        self._apply_theme()

    def _apply_theme(self):
        self.dark = self._resolve_dark()
        sv_ttk.set_theme("dark" if self.dark else "light", self.root)
        # смена темы сбрасывает кастомные стили ttk — задаём заново
        style = ttk.Style(self.root)
        style.configure(
            "Peers.Treeview", rowheight=self._px(36), font=("Segoe UI", 11)
        )
        self._recolor()
        self._apply_window_effects()

    def _recolor(self):
        """Цвета виджетов, которые ttk-тема не красит сама."""
        muted = "#9a9a9a" if self.dark else "#6b6b6b"
        self._device_label.config(foreground=muted)
        self._version_label.config(foreground=muted)
        self.peer_tree.tag_configure(
            "muted", foreground="#8a8a8a" if self.dark else "#7a7a7a"
        )
        self.log_text.config(
            bg="#1f1f1f" if self.dark else "#fbfbfb",
            fg="#d6d6d6" if self.dark else "#333333",
        )

    def _apply_language(self):
        value = self._lang_var.get()
        i18n.set_language(
            i18n.detect_system_language() if value == "auto" else value
        )

    def _set_language(self, value: str):
        self.settings["lang"] = value
        config.save_settings(self.settings)
        self._apply_language()
        self._retranslate()

    def _retranslate(self):
        """Обновить подписи виджетов после смены языка."""
        self._peers_label.config(text=tr("peers"))
        self._log_label.config(text=tr("log"))
        self._btn_clip.config(text=f"📋  {tr('btn_clipboard')}")
        self._btn_files.config(text=f"📁  {tr('btn_files')}")
        self._shown = None  # перерисовать список (плейсхолдер, «старая версия»)
        self.tray.menu = self._tray_menu()
        try:
            self.tray.update_menu()
        except Exception:
            pass  # трей мог ещё не запуститься

    def _open_downloads(self):
        config.DOWNLOADS_DIR.mkdir(parents=True, exist_ok=True)
        os.startfile(config.DOWNLOADS_DIR)

    def _change_folder(self):
        chosen = filedialog.askdirectory(
            parent=self.root, title=tr("folder_dialog_title"),
            initialdir=str(config.DOWNLOADS_DIR),
        )
        if not chosen:
            return
        config.DOWNLOADS_DIR = Path(chosen)
        self.settings["download_dir"] = chosen
        config.save_settings(self.settings)
        self._log(tr("log_downloads", dir=config.DOWNLOADS_DIR))

    # --- обновление по сети ---

    def _rollout_update(self):
        if not getattr(sys, "frozen", False):
            messagebox.showinfo(
                config.APP_NAME, tr("rollout_exe_only"), parent=self.root
            )
            return
        mine = config.version_tuple(config.VERSION)
        targets = [
            p for p in self.disco.get_peers()
            if config.version_tuple(p.version) < mine
        ]
        if not targets:
            messagebox.showinfo(
                config.APP_NAME,
                tr("rollout_all_current", version=config.VERSION),
                parent=self.root,
            )
            return
        names = ", ".join(p.name for p in targets)
        if not messagebox.askyesno(
            config.APP_NAME,
            tr("rollout_confirm", version=config.VERSION, names=names),
            parent=self.root,
        ):
            return
        exe = Path(sys.executable)

        def worker():
            for p in targets:
                try:
                    self._ui(self._log, tr("rollout_sending", name=p.name))
                    transfer.send_update(
                        p.ip, p.port, exe, config.VERSION, self.disco.my_name
                    )
                    self._ui(self._log, tr("rollout_ok", name=p.name, version=config.VERSION))
                except Exception as e:
                    self._ui(self._log, tr("rollout_fail", name=p.name, error=e))

        threading.Thread(target=worker, daemon=True).start()

    def _on_update_received(self, path: Path, version: str, sender: str):
        def apply():
            if config.version_tuple(version) <= config.version_tuple(config.VERSION):
                self._log(tr(
                    "update_skip_old",
                    version=version, name=sender, current=config.VERSION,
                ))
                path.unlink(missing_ok=True)
                return
            if not getattr(sys, "frozen", False):
                self._log(tr("update_skip_noexe", version=version, name=sender))
                path.unlink(missing_ok=True)
                return
            self._log(tr("update_received", version=version, name=sender))
            self._notify(tr("notify_update", version=version, name=sender))
            self._apply_update(path)
        self._ui(apply)

    def _apply_update(self, new_exe: Path):
        """Заменить свой exe и перезапуститься: работающий exe нельзя
        перезаписать, поэтому это делает bat-скрипт после нашего выхода."""
        target = Path(sys.executable)
        bat = Path(tempfile.gettempdir()) / "bufernet_update.bat"
        script = (
            "@echo off\r\n"
            ":retry\r\n"
            "ping -n 2 127.0.0.1 >nul\r\n"
            f'copy /y "{new_exe}" "{target}" >nul 2>&1\r\n'
            "if errorlevel 1 goto retry\r\n"
            f'start "" "{target}"\r\n'
            f'del "{new_exe}"\r\n'
            'del "%~f0"\r\n'
        )
        # cmd читает bat в OEM-кодировке (cp866 на русской Windows)
        try:
            bat.write_text(script, encoding="cp866", newline="")
        except UnicodeEncodeError:
            bat.write_text(script, encoding="utf-8", newline="")
        # чистим служебные переменные PyInstaller: иначе новый exe примет себя
        # за процесс распаковки старого и умрёт, не найдя его временной папки
        env = {
            k: v for k, v in os.environ.items()
            if not k.startswith(("_PYI", "_MEI"))
        }
        subprocess.Popen(
            ["cmd", "/c", str(bat)],
            creationflags=subprocess.CREATE_NO_WINDOW,
            env=env,
            cwd=str(target.parent),
        )
        self._quit()

    # --- запуск ---

    def run(self):
        self.server.start()
        self.disco.start()
        threading.Thread(target=self.tray.run, daemon=True).start()
        self._log(tr(
            "log_start",
            app=config.APP_NAME, version=config.VERSION,
            name=self.disco.my_name, port=self.server.port,
        ))
        self._log(tr("log_downloads", dir=config.DOWNLOADS_DIR))
        self.root.after(200, self._poll)
        self.root.mainloop()

    def _poll(self):
        """Каждые 200 мс: выполняем задачи из фоновых потоков и обновляем список."""
        while True:
            try:
                func, args = self._ui_queue.get_nowait()
            except queue.Empty:
                break
            func(*args)
        self._refresh_peers()
        self.root.after(200, self._poll)

    def _ui(self, func, *args):
        """Запланировать вызов func в потоке GUI (безопасно из любого потока)."""
        self._ui_queue.put((func, args))

    # --- интерфейс ---

    def _build_ui(self):
        px = self._px
        main = ttk.Frame(self.root, padding=(px(16), px(12), px(16), px(16)))
        main.pack(fill="both", expand=True)

        # шапка: название, версия, имя устройства, настройки
        header = ttk.Frame(main)
        header.pack(fill="x")
        ttk.Label(
            header, text=config.APP_NAME,
            font=("Segoe UI Variable Display", 20, "bold"),
        ).pack(side="left")
        self._version_label = ttk.Label(
            header, text=f"v{config.VERSION}", font=("Segoe UI", 10),
        )
        self._version_label.pack(side="left", padx=(px(6), 0), pady=(14, 0))
        self._settings_btn = ttk.Button(
            header, text="⚙", width=3, command=self._show_settings_menu,
        )
        self._settings_btn.pack(side="right", pady=(6, 0))
        self._device_label = ttk.Label(
            header, text=f"💻 {self.disco.my_name}", font=("Segoe UI", 10),
        )
        self._device_label.pack(side="right", pady=(10, 0), padx=(0, px(10)))

        self._peers_label = ttk.Label(
            main, text=tr("peers"), font=("Segoe UI Semibold", 11),
        )
        self._peers_label.pack(anchor="w", pady=(14, 6))

        # список компов — Treeview, чтобы был в стиле Win11
        style = ttk.Style(self.root)
        style.configure("Peers.Treeview", rowheight=px(36), font=("Segoe UI", 11))
        self.peer_tree = ttk.Treeview(
            main, show="tree", height=5, selectmode="browse",
            style="Peers.Treeview",
        )
        self.peer_tree.pack(fill="x")

        btns = ttk.Frame(main)
        btns.pack(fill="x", pady=(12, 0))
        self._btn_clip = ttk.Button(
            btns, text=f"📋  {tr('btn_clipboard')}", style="Accent.TButton",
            command=self._send_clipboard,
        )
        self._btn_clip.pack(side="left", expand=True, fill="x", padx=(0, 8), ipady=2)
        self._btn_files = ttk.Button(
            btns, text=f"📁  {tr('btn_files')}", command=self._send_files,
        )
        self._btn_files.pack(side="left", expand=True, fill="x", ipady=2)

        self._log_label = ttk.Label(
            main, text=tr("log"), font=("Segoe UI Semibold", 11),
        )
        self._log_label.pack(anchor="w", pady=(16, 6))

        log_frame = ttk.Frame(main)
        log_frame.pack(fill="both", expand=True)
        self.log_text = tk.Text(
            log_frame, height=8, state="disabled", wrap="word",
            font=("Segoe UI", 10), relief="flat", highlightthickness=0,
            padx=px(10), pady=px(8),
        )
        self.log_text.pack(side="left", fill="both", expand=True)
        scroll = ttk.Scrollbar(log_frame, command=self.log_text.yview)
        scroll.pack(side="right", fill="y")
        self.log_text.config(yscrollcommand=scroll.set)

        # None, а не [] — чтобы первое же обновление отрисовало плейсхолдер
        self._shown: list[tuple] | None = None
        self._peers_by_id: dict[str, discovery.Peer] = {}

        self._recolor()

    def _refresh_peers(self):
        peers = self.disco.get_peers()
        current = [(p.peer_id, p.name, p.ip, p.version) for p in peers]
        if current == self._shown:
            self._peers_by_id = {p.peer_id: p for p in peers}
            return
        selected = self.peer_tree.selection()
        self.peer_tree.delete(*self.peer_tree.get_children())
        if not peers:
            self.peer_tree.insert(
                "", "end", iid="__none__",
                text=f"  {tr('searching')}", tags=("muted",),
            )
        for p in peers:
            ver = f"v{p.version}" if p.version else tr("old_version")
            self.peer_tree.insert(
                "", "end", iid=p.peer_id,
                text=f"  💻  {p.name}    {p.ip}    ·  {ver}",
            )
        alive_ids = {p.peer_id for p in peers}
        if selected and selected[0] in alive_ids:
            self.peer_tree.selection_set(selected[0])
        elif len(peers) == 1:
            # единственный комп в сети — выбираем его сразу
            self.peer_tree.selection_set(peers[0].peer_id)
        self._shown = current
        self._peers_by_id = {p.peer_id: p for p in peers}

    def _selected_peer(self) -> discovery.Peer | None:
        selected = self.peer_tree.selection()
        peer = self._peers_by_id.get(selected[0]) if selected else None
        if not peer:
            messagebox.showinfo(config.APP_NAME, tr("select_peer"), parent=self.root)
            return None
        return peer

    def _log(self, msg):
        self.log_text.config(state="normal")
        self.log_text.insert("end", str(msg) + "\n")
        self.log_text.see("end")
        self.log_text.config(state="disabled")

    # --- отправка ---

    def _send_clipboard(self):
        peer = self._selected_peer()
        if not peer:
            return
        text = pyperclip.paste()
        if not text:
            messagebox.showinfo(config.APP_NAME, tr("clipboard_empty"), parent=self.root)
            return

        def worker():
            try:
                transfer.send_clipboard(peer.ip, peer.port, text, self.disco.my_name)
                self._ui(self._log, tr("clip_sent", name=peer.name))
            except Exception as e:
                self._ui(self._log, tr("clip_send_fail", name=peer.name, error=e))

        threading.Thread(target=worker, daemon=True).start()

    def _send_files(self):
        peer = self._selected_peer()
        if not peer:
            return
        filenames = filedialog.askopenfilenames(
            parent=self.root, title=tr("file_dialog_title")
        )
        if not filenames:
            return
        paths = [Path(f) for f in filenames]

        def worker():
            try:
                self._ui(self._log, tr("sending_files", count=len(paths), name=peer.name))
                transfer.send_files(
                    peer.ip, peer.port, paths, self.disco.my_name,
                    on_progress=lambda file, size: self._ui(
                        self._log,
                        tr("file_sent", file=file, size=transfer.fmt_size(size)),
                    ),
                )
                self._ui(self._log, tr("files_done", name=peer.name))
            except Exception as e:
                self._ui(self._log, tr("files_fail", name=peer.name, error=e))

        threading.Thread(target=worker, daemon=True).start()

    # --- приём (вызывается из потоков сервера) ---

    def _on_clipboard_received(self, text: str, sender: str):
        def apply():
            pyperclip.copy(text)
            preview = text.strip().replace("\n", " ")
            if len(preview) > 60:
                preview = preview[:60] + "…"
            self._log(tr("clip_received", name=sender, preview=preview))
            self._notify(tr("notify_clip", name=sender))
        self._ui(apply)

    def _on_file_received(self, path: Path, sender: str):
        def apply():
            self._log(tr("file_received", name=sender, file=path))
            self._notify(tr("notify_file", file=path.name, name=sender))
        self._ui(apply)

    def _notify(self, msg: str):
        try:
            self.tray.notify(msg, config.APP_NAME)
        except Exception:
            pass  # уведомления не критичны

    # --- трей / выход ---

    def _hide_window(self):
        self.root.withdraw()
        self._notify(tr("tray_minimized"))

    def _show_window(self):
        self.root.deiconify()
        self.root.lift()
        self.root.focus_force()

    def _quit(self):
        self.disco.stop()
        self.server.stop()
        self.tray.stop()
        self.root.destroy()


def main():
    App().run()
