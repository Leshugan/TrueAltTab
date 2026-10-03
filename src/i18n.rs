// Языки программы: русский, английский, испанский.
use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(1);

/// "auto" — по языку Windows; "ru", "en", "es" — вручную.
pub fn set(setting: &str) {
    let l = match setting.trim().to_lowercase().as_str() {
        "ru" => 0,
        "en" => 1,
        "es" => 2,
        _ => {
            let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() } & 0x3FF;
            match id {
                0x19 | 0x22 | 0x23 => 0, // русский, украинский, белорусский
                0x0A => 2,               // испанский
                _ => 1,
            }
        }
    };
    LANG.store(l, Ordering::Relaxed);
}

pub fn t(key: &str) -> &'static str {
    let l = LANG.load(Ordering::Relaxed) as usize;
    for (k, v) in TABLE {
        if *k == key {
            return v[l];
        }
    }
    "?"
}

/// Подставляет аргументы вместо {} по порядку.
pub fn f(key: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut it = args.iter();
    let mut rest = t(key);
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        out.push_str(it.next().copied().unwrap_or(""));
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

const TABLE: &[(&str, [&str; 3])] = &[
    // ---- меню и значок ----
    ("menu_pause", ["Пауза", "Pause", "Pausa"]),
    ("menu_autostart", ["Запускать вместе с Windows", "Start with Windows", "Iniciar con Windows"]),
    ("menu_log_on", ["Вести журнал", "Keep a log", "Guardar registro"]),
    ("menu_log_open", ["Открыть журнал", "Open log", "Abrir registro"]),
    ("menu_exit", ["Выход", "Exit", "Salir"]),
    ("tip_paused", ["пауза", "paused", "en pausa"]),
    // ---- запуск и меню ----
    ("started", ["===== TrueAltTab {} запущена =====", "===== TrueAltTab {} started =====", "===== TrueAltTab {} iniciado ====="]),
    ("soft_list", ["Прятать без сворачивания: {}", "Hide without minimizing: {}", "Ocultar sin minimizar: {}"]),
    ("exit", ["Выход", "Exit", "Salida"]),
    ("err_input", ["ОШИБКА: не удалось подписаться на клавиатуру и мышь", "ERROR: could not subscribe to keyboard and mouse", "ERROR: no se pudo suscribir al teclado y al ratón"]),
    ("err_hooks", ["ОШИБКА: не удалось подписаться на смену окон", "ERROR: could not subscribe to window changes", "ERROR: no se pudo suscribir a los cambios de ventana"]),
    ("auto_off", ["Автозапуск выключен", "Autostart disabled", "Inicio automático desactivado"]),
    ("auto_on", ["Автозапуск включён", "Autostart enabled", "Inicio automático activado"]),
    ("auto_fail", ["Не удалось включить автозапуск", "Could not enable autostart", "No se pudo activar el inicio automático"]),
    ("log_on", ["Журнал включён", "Log enabled", "Registro activado"]),
    ("log_off", ["Журнал выключен", "Log disabled", "Registro desactivado"]),
    ("pause_on", ["Пауза включена", "Paused", "Pausa activada"]),
    ("pause_off", ["Пауза выключена", "Resumed", "Pausa desactivada"]),
    // ---- работа ----
    ("click", ["клик мышью", "mouse click", "clic del ratón"]),
    ("refocus_fail", ["   не удалось вернуть фокус игре", "   could not return focus to the game", "   no se pudo devolver el foco al juego"]),
    ("hid_soft", ["   спрятал, не сворачивая: {}", "   hidden without minimizing: {}", "   oculto sin minimizar: {}"]),
    ("minimized", ["   свернул: {} — {}", "   minimized: {} — {}", "   minimizado: {} — {}"]),
    ("left_full", ["не свернулась, а вышла в окно — считаю спрятанной", "didn't minimize but left fullscreen — treating it as hidden", "no se minimizó, pero salió de pantalla completa — la considero oculta"]),
    ("stuck", ["не сворачивается ни одним способом — спрятал, забрав у неё фокус", "won't minimize at all — hid it by taking its focus away", "no se minimiza de ninguna forma — la oculté quitándole el foco"]),
    ("crashed", ["   {} закрылась сразу после сворачивания — похоже, не переносит его. Дальше буду прятать её, не сворачивая", "   {} closed right after minimizing — it seems it can't handle it. From now on it will be hidden without minimizing", "   {} se cerró justo después de minimizar — parece que no lo soporta. A partir de ahora se ocultará sin minimizar"]),
    ("already_min", ["   игра уже свернулась сама — держу свёрнутой", "   the game minimized by itself — keeping it down", "   el juego se minimizó solo — lo mantengo minimizado"]),
    ("fights", ["   {} лезет обратно без остановки — перестаю сворачивать, прячу без сворачивания", "   {} keeps coming back nonstop — switching to hiding without minimizing", "   {} vuelve sin parar — dejo de minimizar y lo oculto sin minimizar"]),
    ("user_opened", ["Ты сам открыл игру: {}", "You opened the game yourself: {}", "Abriste el juego tú mismo: {}"]),
    ("user_restored", ["Ты сам развернул игру: {}", "You restored the game yourself: {}", "Restauraste el juego tú mismo: {}"]),
    ("popped", ["Игра сама вылезла — прячу обратно", "The game came back by itself — hiding it again", "El juego volvió solo — lo oculto de nuevo"]),
    ("restored_self", ["Игра развернулась сама — сворачиваю обратно", "The game restored itself — minimizing it again", "El juego se restauró solo — lo minimizo de nuevo"]),
    ("bogus", ["Баг Windows 11: служебное окно ввода забрало фокус у игры — вернул{}", "Windows 11 bug: a hidden input window took focus from the game — returned it{}", "Error de Windows 11: una ventana de entrada oculta quitó el foco al juego — lo devolví{}"]),
    ("bogus_more", [" (и ещё {} раз за минуту)", " (and {} more times in the last minute)", " (y {} veces más en el último minuto)"]),
    ("overlay", ["Поверх игры открылось {} — игру не трогаю", "{} opened over the game — leaving the game alone", "{} se abrió sobre el juego — no toco el juego"]),
    ("leave", ["{} из игры {} → {}", "{} from game {} → {}", "{} desde el juego {} → {}"]),
    ("thief_pause", ["{} упорно забирает фокус — оставляю её в покое на минуту", "{} keeps stealing focus — leaving it alone for a minute", "{} sigue robando el foco — lo dejo en paz un minuto"]),
    ("thief", ["{} забрала фокус у игры без твоего действия — вернул игре", "{} took focus from the game without your action — returned it to the game", "{} quitó el foco al juego sin acción tuya — se lo devolví al juego"]),
    ("game_seen", ["Вижу игру: {}{}", "Game detected: {}{}", "Juego detectado: {}{}"]),
    ("soft_mark", [" (прячу без сворачивания)", " (hide without minimizing)", " (ocultar sin minimizar)"]),
    ("desktop_switch", ["Ctrl+Alt+Del / экран блокировки — сворачиваю {}", "Ctrl+Alt+Del / lock screen — minimizing {}", "Ctrl+Alt+Supr / pantalla de bloqueo — minimizando {}"]),
    ("protection", ["Какая-то программа отключила защиту Windows от кражи фокуса — включил обратно", "Some program turned off Windows' focus-stealing protection — turned it back on", "Algún programa desactivó la protección de Windows contra el robo de foco — la volví a activar"]),
    // ---- способы сворачивания ----
    ("how_gone", ["окна уже нет", "window is gone", "la ventana ya no existe"]),
    ("how_already", ["уже свёрнута", "already minimized", "ya minimizada"]),
    ("how_normal", ["обычное сворачивание", "normal minimize", "minimizado normal"]),
    ("how_command", ["команда «свернуть»", "minimize command", "comando «minimizar»"]),
    ("how_force", ["принудительное сворачивание", "forced minimize", "minimizado forzado"]),
    ("how_back", ["убрал назад и свернул", "sent to back and minimized", "enviada atrás y minimizada"]),
    // ---- файл настроек ----
    ("ini_header", [
        "; TrueAltTab — настройки\r\n\
; Log = yes / no — вести ли журнал TrueAltTab.log на рабочем столе.\r\n\
; Language = auto / ru / en / es — язык программы (auto — как в Windows).\r\n\
; [Ignore] — программы, которые никогда не трогать (имя exe, по одному в строке).\r\n\
; [Soft] — игры, которые плохо переносят сворачивание: их не сворачиваем, а прячем под окна и забираем у них фокус.\r\n\
;          Этот список программа заполняет сама.\r\n\r\n",
        "; TrueAltTab — settings\r\n\
; Log = yes / no — keep the TrueAltTab.log file on the desktop.\r\n\
; Language = auto / ru / en / es — program language (auto = same as Windows).\r\n\
; [Ignore] — programs that are never touched (exe name, one per line).\r\n\
; [Soft] — games that don't handle minimizing well: they are hidden behind other windows and lose focus instead.\r\n\
;          The program fills this list by itself.\r\n\r\n",
        "; TrueAltTab — ajustes\r\n\
; Log = yes / no — guardar el registro TrueAltTab.log en el escritorio.\r\n\
; Language = auto / ru / en / es — idioma del programa (auto = el de Windows).\r\n\
; [Ignore] — programas que nunca se tocan (nombre del exe, uno por línea).\r\n\
; [Soft] — juegos que no soportan bien minimizarse: se ocultan detrás de otras ventanas y pierden el foco.\r\n\
;          El programa rellena esta lista solo.\r\n\r\n",
    ]),
];
