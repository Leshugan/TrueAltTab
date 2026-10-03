#![windows_subsystem = "windows"]
// TrueAltTab — сворачивает игры намертво.

mod core;
mod i18n;
mod store;
mod win;

use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicIsize, Ordering};
use i18n::{f, t as tr};
use win::wide;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const VERSION: &str = "1.000";
const WM_TRAY: u32 = WM_APP + 1;
const TIMER_POLL: usize = 1;
const CMD_PAUSE: usize = 10;
const CMD_LOG: usize = 11;
const CMD_AUTOSTART: usize = 12;
const CMD_EXIT: usize = 13;
const CMD_LOG_ON: usize = 14;
const TASK_NAME: &str = "TrueAltTab";

const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_SYSTEM_MINIMIZEEND: u32 = 0x0017;
const EVENT_SYSTEM_DESKTOPSWITCH: u32 = 0x0020;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicIsize = AtomicIsize::new(0);

/// Обработать нажатия, которые уже ждут в очереди, до того как решать про смену окна.
pub fn drain_raw_input() {
    let hwnd = MAIN_HWND.load(Ordering::Relaxed);
    if hwnd == 0 {
        return;
    }
    unsafe {
        let mut msg: MSG = zeroed();
        while PeekMessageW(&mut msg, hwnd, WM_INPUT, WM_INPUT, PM_REMOVE) != 0 {
            core::on_raw_input(msg.lParam, msg.time);
            DefWindowProcW(hwnd, msg.message, msg.wParam, msg.lParam);
        }
    }
}

unsafe extern "system" fn win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    _id_child: i32,
    _thread: u32,
    time: u32,
) {
    match event {
        EVENT_SYSTEM_FOREGROUND if id_object == 0 => core::on_foreground(hwnd, time),
        EVENT_SYSTEM_MINIMIZEEND if id_object == 0 => core::on_restored(hwnd, time),
        EVENT_SYSTEM_DESKTOPSWITCH => core::on_desktop_switch(time),
        _ => {}
    }
}

fn tray(hwnd: HWND, msg: u32) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        let hinst = GetModuleHandleW(std::ptr::null());
        nid.hIcon = LoadImageW(
            hinst,
            1 as *const u16,
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            0,
        );
        let paused = core::STATE.lock().unwrap().as_ref().map_or(false, |s| s.paused);
        let tip = if paused { format!("TrueAltTab {} — {}", VERSION, tr("tip_paused")) } else { format!("TrueAltTab {}", VERSION) };
        for (i, c) in tip.encode_utf16().take(127).enumerate() {
            nid.szTip[i] = c;
        }
        Shell_NotifyIconW(msg, &nid);
    }
}

fn run_hidden(exe: &str, args: &[&str]) -> bool {
    std::process::Command::new(exe)
        .args(args)
        .creation_flags(0x0800_0000) // без окна консоли
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn autostart_enabled() -> bool {
    run_hidden("schtasks.exe", &["/Query", "/TN", TASK_NAME])
}

/// Автозапуск через Планировщик заданий: с правами администратора и без ограничения «только от сети».
fn set_autostart(on: bool) {
    if !on {
        run_hidden("schtasks.exe", &["/Delete", "/TN", TASK_NAME, "/F"]);
        store::log(tr("auto_off"));
        return;
    }
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    let user = format!(
        "{}\\{}",
        std::env::var("USERDOMAIN").unwrap_or_default(),
        std::env::var("USERNAME").unwrap_or_default()
    );
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <IdleSettings><StopOnIdleEnd>false</StopOnIdleEnd><RestartOnIdle>false</RestartOnIdle></IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>{exe}</Command></Exec></Actions>
</Task>"#,
        user = xml_escape(&user),
        exe = xml_escape(&exe)
    );
    let path = std::env::temp_dir().join("TrueAltTab_task.xml");
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for c in xml.encode_utf16() {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    let _ = std::fs::write(&path, bytes);
    let ok = run_hidden("schtasks.exe", &["/Create", "/TN", TASK_NAME, "/XML", &path.display().to_string(), "/F"]);
    let _ = std::fs::remove_file(&path);
    store::log(tr(if ok { "auto_on" } else { "auto_fail" }));
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn show_menu(hwnd: HWND) {
    unsafe {
        let (paused, log_on) = core::STATE.lock().unwrap().as_ref().map_or((false, true), |s| (s.paused, s.cfg.log));
        let auto = autostart_enabled();
        let m = CreatePopupMenu();
        let add = |id: usize, text: &str, checked: bool, enabled: bool| {
            let w = wide(text);
            let f = MF_STRING | if checked { MF_CHECKED } else { 0 } | if enabled { 0 } else { MF_GRAYED };
            AppendMenuW(m, f, id, w.as_ptr());
        };
        add(CMD_PAUSE, tr("menu_pause"), paused, true);
        add(CMD_AUTOSTART, tr("menu_autostart"), auto, true);
        add(CMD_LOG_ON, tr("menu_log_on"), log_on, true);
        add(CMD_LOG, tr("menu_log_open"), false, log_on);
        AppendMenuW(m, MF_SEPARATOR, 0, std::ptr::null());
        add(CMD_EXIT, tr("menu_exit"), false, true);
        let mut pt = zeroed();
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, std::ptr::null());
        DestroyMenu(m);
        PostMessageW(hwnd, WM_NULL, 0, 0);
        match cmd as usize {
            CMD_PAUSE => {
                core::toggle_pause();
                tray(hwnd, NIM_MODIFY);
            }
            CMD_AUTOSTART => set_autostart(!auto),
            CMD_LOG_ON => core::toggle_log(),
            CMD_LOG => {
                let p = wide(&store::log_path().display().to_string());
                let op = wide("open");
                ShellExecuteW(0, op.as_ptr(), p.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL);
            }
            CMD_EXIT => {
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            core::on_raw_input(lp, GetMessageTime() as u32);
            return DefWindowProcW(hwnd, msg, wp, lp);
        }
        WM_TIMER if wp == TIMER_POLL => {
            core::poll();
            return 0;
        }
        WM_TRAY => {
            let ev = (lp as u32) & 0xFFFF;
            if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                show_menu(hwnd);
            }
            return 0;
        }
        WM_DESTROY => {
            tray(hwnd, NIM_DELETE);
            store::log(tr("exit"));
            PostQuitMessage(0);
            return 0;
        }
        _ => {}
    }
    if msg != 0 && msg as isize == TASKBAR_CREATED.load(Ordering::Relaxed) {
        // Панель задач перезапустилась — возвращаем значок.
        tray(hwnd, NIM_ADD);
        return 0;
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn main() {
    unsafe {
        let mname = wide("Local\\TrueAltTab_SingleInstance");
        CreateMutexW(std::ptr::null(), 1, mname.as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return; // уже запущена
        }

        store::init_log();
        i18n::set("auto");
        let cfg = store::load_config();
        i18n::set(&cfg.lang);
        store::set_log_enabled(cfg.log);
        store::log(&f("started", &[VERSION]));
        store::log(&f("soft_list", &[&if cfg.soft.is_empty() { "—".to_string() } else { cfg.soft.join(", ") }]));
        core::init(cfg);

        let hinst = GetModuleHandleW(std::ptr::null());
        let cls = wide("TrueAltTabWnd");
        let mut wc: WNDCLASSEXW = zeroed();
        wc.cbSize = size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wnd_proc);
        wc.hInstance = hinst;
        wc.lpszClassName = cls.as_ptr();
        RegisterClassExW(&wc);
        let title = wide("TrueAltTab");
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            cls.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            0, 0, 0, 0,
            0, 0, hinst, std::ptr::null(),
        );
        MAIN_HWND.store(hwnd, Ordering::Relaxed);
        let tc = wide("TaskbarCreated");
        TASKBAR_CREATED.store(RegisterWindowMessageW(tc.as_ptr()) as isize, Ordering::Relaxed);

        // Клавиатура и мышь — только время нажатий, ничего не перехватывается и не блокируется.
        let devices = [
            RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd },
            RAWINPUTDEVICE { usUsagePage: 1, usUsage: 6, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd },
        ];
        if RegisterRawInputDevices(devices.as_ptr(), 2, size_of::<RAWINPUTDEVICE>() as u32) == 0 {
            store::log(tr("err_input"));
        }

        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let hooks = [
            SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, 0, Some(win_event), 0, 0, flags),
            SetWinEventHook(EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZEEND, 0, Some(win_event), 0, 0, flags),
            SetWinEventHook(EVENT_SYSTEM_DESKTOPSWITCH, EVENT_SYSTEM_DESKTOPSWITCH, 0, Some(win_event), 0, 0, flags),
        ];
        if hooks.iter().any(|&h| h == 0) {
            store::log(tr("err_hooks"));
        }

        SetTimer(hwnd, TIMER_POLL, 15, None);
        tray(hwnd, NIM_ADD);

        let mut msg: MSG = zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        for h in hooks {
            if h != 0 {
                UnhookWinEvent(h);
            }
        }
    }
}
