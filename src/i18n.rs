//! Локализация интерфейса: en, ru, fr, de, es (порт bufernet/i18n.py).

use std::sync::atomic::{AtomicUsize, Ordering};

pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("ru", "Русский"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("es", "Español"),
];

static CURRENT: AtomicUsize = AtomicUsize::new(0); // индекс в LANGUAGES

pub fn set_language(code: &str) {
    let idx = LANGUAGES.iter().position(|(c, _)| *c == code).unwrap_or(0);
    CURRENT.store(idx, Ordering::Relaxed);
}

pub fn current() -> &'static str {
    LANGUAGES[CURRENT.load(Ordering::Relaxed)].0
}

/// Язык интерфейса Windows -> наш код языка.
pub fn detect_system_language() -> &'static str {
    let langid = unsafe {
        windows::Win32::Globalization::GetUserDefaultUILanguage()
    };
    match langid & 0x3FF {
        0x09 => "en",
        0x19 => "ru",
        0x0C => "fr",
        0x07 => "de",
        0x0A => "es",
        _ => "en",
    }
}

/// Перевод без подстановок.
pub fn tr(key: &str) -> String {
    lookup(current(), key)
        .or_else(|| lookup("en", key))
        .unwrap_or(key)
        .to_string()
}

/// Перевод с подстановкой плейсхолдеров: trf("clip_sent", &[("name", "PC-1")]).
pub fn trf(key: &str, args: &[(&str, &str)]) -> String {
    let mut s = tr(key);
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

fn lookup(lang: &str, key: &str) -> Option<&'static str> {
    let table = match lang {
        "ru" => RU,
        "fr" => FR,
        "de" => DE,
        "es" => ES,
        _ => EN,
    };
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

type Table = &'static [(&'static str, &'static str)];

static EN: Table = &[
    ("peers", "Computers on the network"),
    ("searching", "Searching for computers on the network…"),
    ("old_version", "old version"),
    ("btn_clipboard", "Send clipboard"),
    ("btn_files", "Send files…"),
    ("btn_folder", "Folder"),
    ("log", "Log"),
    ("tray_show", "Show window"),
    ("tray_exit", "Exit"),
    ("tray_minimized", "Minimized to tray — still receiving files"),
    ("theme_auto", "Theme: match Windows"),
    ("theme_light", "Light theme"),
    ("theme_dark", "Dark theme"),
    ("menu_language", "Language"),
    ("lang_auto", "Match Windows"),
    ("menu_hotkeys", "Hotkeys"),
    ("hotkey_off", "off"),
    ("hotkey_capture", "Press a new shortcut…\nEsc — cancel, Del — disable"),
    ("hotkey_conflict", "Could not register {combo} — already in use by another app"),
    ("menu_autostart", "Start with Windows"),
    ("menu_rollout", "Roll out update to network"),
    ("menu_open_folder", "Open received files folder"),
    ("menu_change_folder", "Change save folder…"),
    ("select_peer", "Select a computer in the list first."),
    ("clipboard_empty", "The clipboard is empty."),
    ("file_dialog_title", "Which files to send?"),
    ("folder_dialog_title", "Where to save received files?"),
    ("log_start", "{app} v{version} — {name}, port {port}"),
    ("log_downloads", "Received files: {dir}"),
    ("clip_sent", "Clipboard sent to {name}"),
    ("clip_send_fail", "Failed to send clipboard to {name}: {error}"),
    ("sending_files", "Sending {count} file(s) to {name}…"),
    ("file_sent", "Sent: {file} ({size})"),
    ("files_done", "Done: everything sent to {name}"),
    ("files_fail", "Failed to send to {name}: {error}"),
    ("clip_received", "Clipboard from {name}: {preview}"),
    ("notify_clip", "Clipboard received from {name}"),
    ("img_sent", "Image sent to {name}"),
    ("img_received", "Image from {name} ({size}) — now in your clipboard"),
    ("notify_img", "Image received from {name}"),
    ("file_received", "File from {name}: {file}"),
    ("notify_file", "File {file} received from {name}"),
    ("recv_error", "Receive error from {name}: {error}"),
    ("rollout_all_current", "Every computer already has v{version} or newer."),
    (
        "rollout_confirm",
        "Send BuferNet v{version} to: {names}?\nEach computer will replace its exe and restart.",
    ),
    ("rollout_sending", "Sending update to {name}…"),
    ("rollout_ok", "{name} received v{version} and is restarting"),
    ("rollout_fail", "Failed to update {name}: {error}"),
    (
        "update_skip_old",
        "Update v{version} from {name} is not newer than v{current} — skipped",
    ),
    ("update_received", "Received update v{version} from {name} — restarting…"),
    ("notify_update", "Updating to v{version} from {name}"),
    ("err_conn_lost", "connection lost during transfer"),
    (
        "err_no_ack",
        "receiver did not confirm — it may run a version without update support; \
update it manually once",
    ),
];

static RU: Table = &[
    ("peers", "Компьютеры в сети"),
    ("searching", "Поиск компьютеров в сети…"),
    ("old_version", "старая версия"),
    ("btn_clipboard", "Отправить буфер"),
    ("btn_files", "Отправить файлы…"),
    ("btn_folder", "Папка"),
    ("log", "Журнал"),
    ("tray_show", "Показать окно"),
    ("tray_exit", "Выход"),
    ("tray_minimized", "Свёрнуто в трей — файлы принимаются дальше"),
    ("theme_auto", "Тема как в Windows"),
    ("theme_light", "Светлая тема"),
    ("theme_dark", "Тёмная тема"),
    ("menu_language", "Язык"),
    ("lang_auto", "Как в Windows"),
    ("menu_hotkeys", "Горячие клавиши"),
    ("hotkey_off", "выкл"),
    ("hotkey_capture", "Нажмите новое сочетание клавиш…\nEsc — отмена, Del — выключить"),
    ("hotkey_conflict", "Не удалось занять {combo} — сочетание использует другая программа"),
    ("menu_autostart", "Запускать при входе в Windows"),
    ("menu_rollout", "Раскатать обновление на компы в сети"),
    ("menu_open_folder", "Открыть папку принятых файлов"),
    ("menu_change_folder", "Сменить папку сохранения…"),
    ("select_peer", "Сначала выбери компьютер в списке."),
    ("clipboard_empty", "Буфер обмена пуст."),
    ("file_dialog_title", "Какие файлы отправить?"),
    ("folder_dialog_title", "Куда сохранять принятые файлы?"),
    ("log_start", "{app} v{version} — {name}, порт {port}"),
    ("log_downloads", "Принятые файлы: {dir}"),
    ("clip_sent", "Буфер отправлен на {name}"),
    ("clip_send_fail", "Не удалось отправить буфер на {name}: {error}"),
    ("sending_files", "Отправка {count} файл(ов) на {name}…"),
    ("file_sent", "Отправлен: {file} ({size})"),
    ("files_done", "Готово: всё отправлено на {name}"),
    ("files_fail", "Ошибка отправки на {name}: {error}"),
    ("clip_received", "Буфер от {name}: {preview}"),
    ("notify_clip", "Буфер обмена получен от {name}"),
    ("img_sent", "Картинка отправлена на {name}"),
    ("img_received", "Картинка от {name} ({size}) — уже в буфере обмена"),
    ("notify_img", "Картинка получена от {name}"),
    ("file_received", "Файл от {name}: {file}"),
    ("notify_file", "Файл {file} получен от {name}"),
    ("recv_error", "Ошибка приёма от {name}: {error}"),
    ("rollout_all_current", "У всех компьютеров в сети уже v{version} или новее."),
    (
        "rollout_confirm",
        "Отправить BuferNet v{version} на: {names}?\nКаждый комп сам заменит exe и перезапустится.",
    ),
    ("rollout_sending", "Отправка обновления на {name}…"),
    ("rollout_ok", "{name} получил v{version} и перезапускается"),
    ("rollout_fail", "Не удалось обновить {name}: {error}"),
    (
        "update_skip_old",
        "Обновление v{version} от {name} не новее моей v{current} — пропущено",
    ),
    ("update_received", "Получено обновление v{version} от {name} — перезапуск…"),
    ("notify_update", "Обновление до v{version} от {name}"),
    ("err_conn_lost", "соединение оборвалось при передаче"),
    (
        "err_no_ack",
        "получатель не подтвердил приём — возможно, там версия без поддержки \
обновлений, один раз обнови её вручную",
    ),
];

static FR: Table = &[
    ("peers", "Ordinateurs sur le réseau"),
    ("searching", "Recherche d'ordinateurs sur le réseau…"),
    ("old_version", "ancienne version"),
    ("btn_clipboard", "Envoyer le presse-papiers"),
    ("btn_files", "Envoyer des fichiers…"),
    ("btn_folder", "Dossier"),
    ("log", "Journal"),
    ("tray_show", "Afficher la fenêtre"),
    ("tray_exit", "Quitter"),
    ("tray_minimized", "Réduit dans la zone de notification — réception toujours active"),
    ("theme_auto", "Thème : comme Windows"),
    ("theme_light", "Thème clair"),
    ("theme_dark", "Thème sombre"),
    ("menu_language", "Langue"),
    ("lang_auto", "Comme Windows"),
    ("menu_hotkeys", "Raccourcis clavier"),
    ("hotkey_off", "désactivé"),
    ("hotkey_capture", "Appuyez sur un nouveau raccourci…\nÉchap — annuler, Suppr — désactiver"),
    ("hotkey_conflict", "Impossible d'enregistrer {combo} — déjà utilisé par une autre application"),
    ("menu_autostart", "Lancer au démarrage de Windows"),
    ("menu_rollout", "Déployer la mise à jour sur le réseau"),
    ("menu_open_folder", "Ouvrir le dossier des fichiers reçus"),
    ("menu_change_folder", "Changer le dossier d'enregistrement…"),
    ("select_peer", "Sélectionnez d'abord un ordinateur dans la liste."),
    ("clipboard_empty", "Le presse-papiers est vide."),
    ("file_dialog_title", "Quels fichiers envoyer ?"),
    ("folder_dialog_title", "Où enregistrer les fichiers reçus ?"),
    ("log_start", "{app} v{version} — {name}, port {port}"),
    ("log_downloads", "Fichiers reçus : {dir}"),
    ("clip_sent", "Presse-papiers envoyé à {name}"),
    ("clip_send_fail", "Échec de l'envoi du presse-papiers à {name} : {error}"),
    ("sending_files", "Envoi de {count} fichier(s) à {name}…"),
    ("file_sent", "Envoyé : {file} ({size})"),
    ("files_done", "Terminé : tout a été envoyé à {name}"),
    ("files_fail", "Échec de l'envoi à {name} : {error}"),
    ("clip_received", "Presse-papiers de {name} : {preview}"),
    ("notify_clip", "Presse-papiers reçu de {name}"),
    ("img_sent", "Image envoyée à {name}"),
    ("img_received", "Image de {name} ({size}) — déjà dans le presse-papiers"),
    ("notify_img", "Image reçue de {name}"),
    ("file_received", "Fichier de {name} : {file}"),
    ("notify_file", "Fichier {file} reçu de {name}"),
    ("recv_error", "Erreur de réception de {name} : {error}"),
    ("rollout_all_current", "Tous les ordinateurs ont déjà la v{version} ou plus récente."),
    (
        "rollout_confirm",
        "Envoyer BuferNet v{version} à : {names} ?\nChaque ordinateur remplacera son exe et redémarrera.",
    ),
    ("rollout_sending", "Envoi de la mise à jour à {name}…"),
    ("rollout_ok", "{name} a reçu la v{version} et redémarre"),
    ("rollout_fail", "Impossible de mettre à jour {name} : {error}"),
    (
        "update_skip_old",
        "Mise à jour v{version} de {name} pas plus récente que la v{current} — ignorée",
    ),
    ("update_received", "Mise à jour v{version} reçue de {name} — redémarrage…"),
    ("notify_update", "Mise à jour vers v{version} depuis {name}"),
    ("err_conn_lost", "connexion interrompue pendant le transfert"),
    (
        "err_no_ack",
        "le destinataire n'a pas confirmé — version sans prise en charge des mises à jour ? \
Mettez-le à jour manuellement une fois",
    ),
];

static DE: Table = &[
    ("peers", "Computer im Netzwerk"),
    ("searching", "Suche nach Computern im Netzwerk…"),
    ("old_version", "alte Version"),
    ("btn_clipboard", "Zwischenablage senden"),
    ("btn_files", "Dateien senden…"),
    ("btn_folder", "Ordner"),
    ("log", "Protokoll"),
    ("tray_show", "Fenster anzeigen"),
    ("tray_exit", "Beenden"),
    ("tray_minimized", "In den Infobereich minimiert — Empfang läuft weiter"),
    ("theme_auto", "Design: wie Windows"),
    ("theme_light", "Helles Design"),
    ("theme_dark", "Dunkles Design"),
    ("menu_language", "Sprache"),
    ("lang_auto", "Wie Windows"),
    ("menu_hotkeys", "Tastenkürzel"),
    ("hotkey_off", "aus"),
    ("hotkey_capture", "Neue Tastenkombination drücken…\nEsc — Abbrechen, Entf — deaktivieren"),
    ("hotkey_conflict", "{combo} konnte nicht registriert werden — von anderer App belegt"),
    ("menu_autostart", "Mit Windows starten"),
    ("menu_rollout", "Update im Netzwerk verteilen"),
    ("menu_open_folder", "Ordner mit empfangenen Dateien öffnen"),
    ("menu_change_folder", "Speicherordner ändern…"),
    ("select_peer", "Wähle zuerst einen Computer in der Liste aus."),
    ("clipboard_empty", "Die Zwischenablage ist leer."),
    ("file_dialog_title", "Welche Dateien senden?"),
    ("folder_dialog_title", "Wo sollen empfangene Dateien gespeichert werden?"),
    ("log_start", "{app} v{version} — {name}, Port {port}"),
    ("log_downloads", "Empfangene Dateien: {dir}"),
    ("clip_sent", "Zwischenablage an {name} gesendet"),
    ("clip_send_fail", "Zwischenablage konnte nicht an {name} gesendet werden: {error}"),
    ("sending_files", "Sende {count} Datei(en) an {name}…"),
    ("file_sent", "Gesendet: {file} ({size})"),
    ("files_done", "Fertig: alles an {name} gesendet"),
    ("files_fail", "Senden an {name} fehlgeschlagen: {error}"),
    ("clip_received", "Zwischenablage von {name}: {preview}"),
    ("notify_clip", "Zwischenablage von {name} empfangen"),
    ("img_sent", "Bild an {name} gesendet"),
    ("img_received", "Bild von {name} ({size}) — bereits in der Zwischenablage"),
    ("notify_img", "Bild von {name} empfangen"),
    ("file_received", "Datei von {name}: {file}"),
    ("notify_file", "Datei {file} von {name} empfangen"),
    ("recv_error", "Empfangsfehler von {name}: {error}"),
    ("rollout_all_current", "Alle Computer haben bereits v{version} oder neuer."),
    (
        "rollout_confirm",
        "BuferNet v{version} an {names} senden?\nJeder Computer ersetzt seine exe und startet neu.",
    ),
    ("rollout_sending", "Sende Update an {name}…"),
    ("rollout_ok", "{name} hat v{version} erhalten und startet neu"),
    ("rollout_fail", "Update von {name} fehlgeschlagen: {error}"),
    (
        "update_skip_old",
        "Update v{version} von {name} ist nicht neuer als v{current} — übersprungen",
    ),
    ("update_received", "Update v{version} von {name} erhalten — Neustart…"),
    ("notify_update", "Aktualisierung auf v{version} von {name}"),
    ("err_conn_lost", "Verbindung während der Übertragung abgebrochen"),
    (
        "err_no_ack",
        "Empfänger hat nicht bestätigt — evtl. Version ohne Update-Unterstützung; \
einmal manuell aktualisieren",
    ),
];

static ES: Table = &[
    ("peers", "Equipos en la red"),
    ("searching", "Buscando equipos en la red…"),
    ("old_version", "versión antigua"),
    ("btn_clipboard", "Enviar portapapeles"),
    ("btn_files", "Enviar archivos…"),
    ("btn_folder", "Carpeta"),
    ("log", "Registro"),
    ("tray_show", "Mostrar ventana"),
    ("tray_exit", "Salir"),
    ("tray_minimized", "Minimizado a la bandeja — sigue recibiendo archivos"),
    ("theme_auto", "Tema: como Windows"),
    ("theme_light", "Tema claro"),
    ("theme_dark", "Tema oscuro"),
    ("menu_language", "Idioma"),
    ("lang_auto", "Como Windows"),
    ("menu_hotkeys", "Atajos de teclado"),
    ("hotkey_off", "desactivado"),
    ("hotkey_capture", "Pulsa un nuevo atajo…\nEsc — cancelar, Supr — desactivar"),
    ("hotkey_conflict", "No se pudo registrar {combo} — ya lo usa otra aplicación"),
    ("menu_autostart", "Iniciar con Windows"),
    ("menu_rollout", "Distribuir actualización por la red"),
    ("menu_open_folder", "Abrir carpeta de archivos recibidos"),
    ("menu_change_folder", "Cambiar carpeta de guardado…"),
    ("select_peer", "Primero selecciona un equipo de la lista."),
    ("clipboard_empty", "El portapapeles está vacío."),
    ("file_dialog_title", "¿Qué archivos enviar?"),
    ("folder_dialog_title", "¿Dónde guardar los archivos recibidos?"),
    ("log_start", "{app} v{version} — {name}, puerto {port}"),
    ("log_downloads", "Archivos recibidos: {dir}"),
    ("clip_sent", "Portapapeles enviado a {name}"),
    ("clip_send_fail", "No se pudo enviar el portapapeles a {name}: {error}"),
    ("sending_files", "Enviando {count} archivo(s) a {name}…"),
    ("file_sent", "Enviado: {file} ({size})"),
    ("files_done", "Listo: todo enviado a {name}"),
    ("files_fail", "Error al enviar a {name}: {error}"),
    ("clip_received", "Portapapeles de {name}: {preview}"),
    ("notify_clip", "Portapapeles recibido de {name}"),
    ("img_sent", "Imagen enviada a {name}"),
    ("img_received", "Imagen de {name} ({size}) — ya está en el portapapeles"),
    ("notify_img", "Imagen recibida de {name}"),
    ("file_received", "Archivo de {name}: {file}"),
    ("notify_file", "Archivo {file} recibido de {name}"),
    ("recv_error", "Error de recepción de {name}: {error}"),
    ("rollout_all_current", "Todos los equipos ya tienen la v{version} o más reciente."),
    (
        "rollout_confirm",
        "¿Enviar BuferNet v{version} a: {names}?\nCada equipo reemplazará su exe y se reiniciará.",
    ),
    ("rollout_sending", "Enviando actualización a {name}…"),
    ("rollout_ok", "{name} recibió la v{version} y se está reiniciando"),
    ("rollout_fail", "No se pudo actualizar {name}: {error}"),
    (
        "update_skip_old",
        "La actualización v{version} de {name} no es más nueva que la v{current} — omitida",
    ),
    ("update_received", "Actualización v{version} recibida de {name} — reiniciando…"),
    ("notify_update", "Actualizando a v{version} desde {name}"),
    ("err_conn_lost", "conexión perdida durante la transferencia"),
    (
        "err_no_ack",
        "el receptor no confirmó — puede ser una versión sin soporte de actualizaciones; \
actualízalo manualmente una vez",
    ),
];
