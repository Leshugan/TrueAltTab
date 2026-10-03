// Журнал на рабочем столе и файл настроек рядом с exe.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath};

static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
static LOG_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_log_enabled(on: bool) {
    LOG_ON.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn desktop_dir() -> PathBuf {
    unsafe {
        let mut p: windows_sys::core::PWSTR = std::ptr::null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Desktop, 0, 0, &mut p) == 0 && !p.is_null() {
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            CoTaskMemFree(p as *const _);
            return PathBuf::from(s);
        }
    }
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
    PathBuf::from(home).join("Desktop")
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn log_path() -> PathBuf {
    desktop_dir().join("TrueAltTab.log")
}

pub fn init_log() {
    let p = log_path();
    // Не даём журналу разрастись: больше 2 МБ — начинаем заново.
    if let Ok(m) = fs::metadata(&p) {
        if m.len() > 2 * 1024 * 1024 {
            let _ = fs::remove_file(&p);
        }
    }
    *LOG_PATH.lock().unwrap() = Some(p);
}

pub fn log(msg: &str) {
    if !LOG_ON.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let path = match LOG_PATH.lock().unwrap().clone() {
        Some(p) => p,
        None => return,
    };
    let t = unsafe {
        let mut st = std::mem::zeroed();
        GetLocalTime(&mut st);
        st
    };
    let line = format!(
        "{:02}.{:02} {:02}:{:02}:{:02}.{:03}  {}\r\n",
        t.wDay, t.wMonth, t.wHour, t.wMinute, t.wSecond, t.wMilliseconds, msg
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
}

// ---------------- Настройки ----------------

pub struct Config {
    pub log: bool,
    pub lang: String,
    pub ignore: Vec<String>,
    pub soft: Vec<String>,
}

const DEFAULT_IGNORE: &[&str] = &[
    "explorer.exe", "chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "brave.exe",
    "vivaldi.exe", "browser.exe", "vlc.exe", "mpv.exe", "potplayermini64.exe", "mpc-hc64.exe",
    "mpc-be64.exe", "taskmgr.exe",
];

pub fn ini_path() -> PathBuf {
    exe_dir().join("TrueAltTab.ini")
}

pub fn load_config() -> Config {
    let mut cfg = Config { log: true, lang: "auto".into(), ignore: vec![], soft: vec![] };
    let text = match fs::read_to_string(ini_path()) {
        Ok(t) => t,
        Err(_) => {
            cfg.ignore = DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect();
            save_config(&cfg);
            return cfg;
        }
    };
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_lowercase();
            continue;
        }
        let v = line.to_lowercase();
        match section.as_str() {
            // старые русские названия разделов тоже понимаем
            "general" | "общее" => {
                if let Some((k, val)) = v.split_once('=') {
                    let val = val.trim();
                    match k.trim() {
                        "log" | "журнал" => cfg.log = !matches!(val, "no" | "нет" | "0" | "off"),
                        "language" | "язык" => cfg.lang = val.to_string(),
                        _ => {}
                    }
                }
            }
            "ignore" | "не трогать" => cfg.ignore.push(v),
            "soft" | "мягко" => cfg.soft.push(v),
            _ => {}
        }
    }
    // Файл старого формата (русские названия разделов) — переписываем в новый.
    if text.contains("[Не трогать]") || text.contains("[Мягко]") || text.contains("[Общее]") {
        save_config(&cfg);
    }
    cfg
}

pub fn save_config(cfg: &Config) {
    let mut s = String::from(crate::i18n::t("ini_header"));
    s.push_str("[General]\r\n");
    s.push_str(if cfg.log { "Log = yes\r\n" } else { "Log = no\r\n" });
    s.push_str(&format!("Language = {}\r\n\r\n", cfg.lang));
    s.push_str("[Ignore]\r\n");
    for e in &cfg.ignore {
        s.push_str(e);
        s.push_str("\r\n");
    }
    s.push_str("\r\n[Soft]\r\n");
    for e in &cfg.soft {
        s.push_str(e);
        s.push_str("\r\n");
    }
    let _ = fs::write(ini_path(), s);
}
