// Вспомогательные функции для работы с окнами Windows.
use std::mem::{size_of, zeroed};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{ClientToScreen, GetMonitorInfoW, MonitorFromWindow, MONITORINFO};
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn tick() -> u32 {
    unsafe { windows_sys::Win32::System::SystemInformation::GetTickCount() }
}

/// Сколько миллисекунд прошло от `earlier` до `later` (с учётом переполнения счётчика).
pub fn elapsed(later: u32, earlier: u32) -> i32 {
    later.wrapping_sub(earlier) as i32
}

#[derive(Clone, Debug, Default)]
pub struct WinInfo {
    pub hwnd: HWND,
    pub root: HWND,
    pub pid: u32,
    pub exe: String,   // имя exe в нижнем регистре
    pub class: String,
    pub title: String,
}

impl WinInfo {
    pub fn describe(&self) -> String {
        let t: String = self.title.chars().take(60).collect();
        format!("{} [{}] «{}»", self.exe, self.class, t)
    }
}

pub fn capture(hwnd: HWND) -> WinInfo {
    unsafe {
        let mut info = WinInfo { hwnd, ..Default::default() };
        info.root = GetAncestor(hwnd, GA_ROOTOWNER);
        let mut buf = [0u16; 512];
        let n = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n > 0 {
            info.class = String::from_utf16_lossy(&buf[..n as usize]);
        }
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n > 0 {
            info.title = String::from_utf16_lossy(&buf[..n as usize]);
        }
        GetWindowThreadProcessId(hwnd, &mut info.pid);
        info.exe = process_exe(info.pid);
        info
    }
}

pub fn process_exe(pid: u32) -> String {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return format!("pid{}", pid);
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return format!("pid{}", pid);
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit('\\').next().unwrap_or("").to_lowercase()
    }
}

pub fn process_alive(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h == 0 {
            return false;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE as u32
    }
}

pub fn is_window(h: HWND) -> bool {
    unsafe { IsWindow(h) != 0 }
}
pub fn is_minimized(h: HWND) -> bool {
    unsafe { IsIconic(h) != 0 }
}
pub fn is_visible(h: HWND) -> bool {
    unsafe { IsWindowVisible(h) != 0 }
}
pub fn is_hung(h: HWND) -> bool {
    unsafe { IsHungAppWindow(h) != 0 }
}
pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}

fn is_cloaked(h: HWND) -> bool {
    unsafe {
        let mut v: u32 = 0;
        // 14 = DWMWA_CLOAKED
        DwmGetWindowAttribute(h, 14, &mut v as *mut u32 as *mut _, 4) == 0 && v != 0
    }
}

pub fn is_topmost(h: HWND) -> bool {
    unsafe { (GetWindowLongPtrW(h, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST) != 0 }
}

/// Части Windows, которые законно получают фокус (Пуск, поиск, переключатель окон и т.п.).
const SYSTEM_EXES: &[&str] = &[
    "startmenuexperiencehost.exe", "searchhost.exe", "searchapp.exe", "searchui.exe",
    "shellexperiencehost.exe", "shellhost.exe", "lockapp.exe", "logonui.exe", "consent.exe",
    "credentialuibroker.exe",
];
const SYSTEM_EXPLORER_CLASSES: &[&str] = &[
    "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "TaskSwitcherWnd", "MultitaskingViewFrame",
    "XamlExplorerHostIslandWindow", "ForegroundStaging", "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland", "Windows.UI.Core.CoreWindow", "#32768",
    "Progman", "WorkerW",
];

/// Панель задач, её превью, переключатель Alt+Tab и Win+Tab — то, откуда пользователь выбирает окно.
pub fn is_taskbar_like(h: HWND) -> bool {
    if h == 0 {
        return false;
    }
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return false;
    }
    let c = String::from_utf16_lossy(&buf[..n as usize]);
    matches!(
        c.as_str(),
        "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "TaskListThumbnailWnd" | "XamlExplorerHostIslandWindow"
            | "MultitaskingViewFrame" | "TaskSwitcherWnd" | "ForegroundStaging"
    )
}

pub fn is_system_ui(w: &WinInfo) -> bool {
    SYSTEM_EXES.contains(&w.exe.as_str())
        || (w.exe == "explorer.exe" && SYSTEM_EXPLORER_CLASSES.contains(&w.class.as_str()))
}

/// Невидимое служебное окно ввода, которому баг Windows 11 отдаёт фокус после кликов.
pub fn is_bogus(w: &WinInfo) -> bool {
    w.class == "MSCTFIME UI" || w.class == "IME"
}

/// Окна поверх игры, которые не должны её сворачивать (скриншоты, Game Bar, панель эмодзи, отчёт об ошибке).
const OVERLAY_EXES: &[&str] = &[
    "screenclippinghost.exe", "snippingtool.exe", "gamebar.exe", "gamebarftserver.exe",
    "textinputhost.exe", "werfault.exe", "nvidia overlay.exe", "nvidia share.exe",
    "applicationframehost.exe",
];

pub fn is_overlay(w: &WinInfo) -> bool {
    OVERLAY_EXES.contains(&w.exe.as_str())
}

/// Закрывает ли окно весь свой монитор (рабочей частью, если у окна есть заголовок).
pub fn covers_monitor(h: HWND) -> bool {
    unsafe {
        let style = GetWindowLongPtrW(h, GWL_STYLE) as u32;
        let mut r: RECT = zeroed();
        if style & WS_CAPTION == WS_CAPTION {
            if GetClientRect(h, &mut r) == 0 {
                return false;
            }
            let mut tl = POINT { x: r.left, y: r.top };
            let mut br = POINT { x: r.right, y: r.bottom };
            ClientToScreen(h, &mut tl);
            ClientToScreen(h, &mut br);
            r = RECT { left: tl.x, top: tl.y, right: br.x, bottom: br.y };
        } else if GetWindowRect(h, &mut r) == 0 {
            return false;
        }
        let mon = MonitorFromWindow(h, 2 /* MONITOR_DEFAULTTONEAREST */);
        let mut mi: MONITORINFO = zeroed();
        mi.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut mi) == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

/// Окно похоже на игру: без заголовка и рамки, закрывает весь СВОЙ монитор.
pub fn looks_like_game(w: &WinInfo, ignore: &[String], own_pid: u32) -> bool {
    let h = w.hwnd;
    if h == 0 || w.pid == own_pid || !is_visible(h) || is_minimized(h) || is_cloaked(h) {
        return false;
    }
    if is_system_ui(w) || is_bogus(w) || is_overlay(w) {
        return false;
    }
    if ignore.iter().any(|e| e == &w.exe) {
        return false;
    }
    let style = unsafe { GetWindowLongPtrW(h, GWL_STYLE) as u32 };
    if style & WS_CHILD != 0 {
        return false;
    }
    covers_monitor(h)
}

fn wait_minimized(h: HWND, ms: u32) -> bool {
    let start = tick();
    loop {
        if !is_window(h) || is_minimized(h) {
            return true;
        }
        if elapsed(tick(), start) >= ms as i32 {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

pub enum Outcome {
    /// Свернулась (ключ строки со способом).
    Minimized(&'static str),
    /// Не свернулась, но вышла из полноэкранного режима и висит окном — считаем спрятанной.
    LeftFullscreen,
    /// Не свернулась и осталась на весь экран — прячем, забирая фокус.
    Stuck,
}

/// Сворачивает окно, переходя ко всё более жёстким способам. Ничего не нажимает за пользователя.
pub fn minimize_ladder(h: HWND) -> Outcome {
    let was_full = covers_monitor(h);
    let (ok, how) = ladder_steps(h);
    if ok {
        return Outcome::Minimized(how);
    }
    if was_full && is_window(h) && !covers_monitor(h) {
        return Outcome::LeftFullscreen;
    }
    Outcome::Stuck
}

fn ladder_steps(h: HWND) -> (bool, &'static str) {
    unsafe {
        if !is_window(h) {
            return (true, "how_gone");
        }
        if is_minimized(h) {
            return (true, "how_already");
        }
        if is_topmost(h) {
            SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        }
        ShowWindowAsync(h, SW_MINIMIZE);
        if wait_minimized(h, 150) {
            return (true, "how_normal");
        }
        PostMessageW(h, WM_SYSCOMMAND, SC_MINIMIZE as usize, 0);
        if wait_minimized(h, 150) {
            return (true, "how_command");
        }
        ShowWindow(h, SW_FORCEMINIMIZE);
        if wait_minimized(h, 200) {
            return (true, "how_force");
        }
        SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        SetWindowPos(h, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        ShowWindowAsync(h, SW_SHOWMINNOACTIVE);
        if wait_minimized(h, 400) {
            return (true, "how_back");
        }
        (false, "")
    }
}

/// Мягкий режим для игр, которые ломаются от сворачивания: не сворачивать, а убрать под все окна.
pub fn push_back(h: HWND) {
    unsafe {
        SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
        SetWindowPos(h, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS);
    }
}

/// Вернуть фокус окну (взято из NoFocusSteal: подключение к вводу активного окна,
/// запасной путь — нажатие клавиши, которую не использует ни одна программа).
pub fn force_foreground(target: HWND) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        let fg_thread = if fg == 0 { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) };
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        BringWindowToTop(target);
        SetForegroundWindow(target);
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
        if GetForegroundWindow() == target {
            return true;
        }
        let mut inputs: [INPUT; 2] = zeroed();
        inputs[0].r#type = INPUT_KEYBOARD;
        inputs[0].Anonymous.ki.wVk = 0xE8;
        inputs[1].r#type = INPUT_KEYBOARD;
        inputs[1].Anonymous.ki.wVk = 0xE8;
        inputs[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        SendInput(2, inputs.as_ptr(), size_of::<INPUT>() as i32);
        SetForegroundWindow(target);
        GetForegroundWindow() == target
    }
}

/// Спрятать окно без сворачивания: убрать под остальные окна и забрать у него фокус
/// (отдать окну, куда ушёл пользователь, или панели задач).
pub fn take_focus_from(game: HWND, prefer: HWND) {
    push_back(game);
    if foreground() != game {
        return;
    }
    let target = if prefer != 0 && prefer != game && is_window(prefer) && is_visible(prefer) && !is_minimized(prefer) {
        prefer
    } else {
        unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) }
    };
    if target != 0 {
        force_foreground(target);
    }
}

pub fn key_down_now(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 }
}

/// Игры (GLFW и др.) при запуске отключают системную защиту от кражи фокуса.
/// Возвращаем её на время работы (без записи в реестр). true — если пришлось вернуть.
pub fn restore_focus_protection() -> bool {
    unsafe {
        let mut v: u32 = 0;
        if SystemParametersInfoW(SPI_GETFOREGROUNDLOCKTIMEOUT, 0, &mut v as *mut u32 as *mut _, 0) == 0 {
            return false;
        }
        if v == 0 {
            SystemParametersInfoW(SPI_SETFOREGROUNDLOCKTIMEOUT, 0, 200000usize as *mut _, 0);
            return true;
        }
        false
    }
}
