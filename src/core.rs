// Главная логика: когда сворачивать игру, когда класть обратно, когда возвращать ей фокус.
use crate::i18n::{f, t as tr};
use crate::store::{self, log, Config};
use crate::win::{self, elapsed, tick, WinInfo};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::Input::{GetRawInputData, HRAWINPUT, RID_INPUT};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetAncestor, GetCursorPos, WindowFromPoint, GA_ROOTOWNER};

/// Сколько миллисекунд после нажатия считается, что действие сделал пользователь.
const INTENT_MS: i32 = 1500;
/// Небольшой запас: часы событий тикают раз в ~16 мс.
const SLACK_MS: i32 = 30;
/// Нажатия раньше этого срока после сворачивания не считаются желанием открыть игру обратно.
const RESTORE_GRACE_MS: i32 = 250;
/// Если игра лезет обратно чаще этого — перестаём сворачивать её и просто убираем назад.
const FIGHT_LIMIT: usize = 20;
const FIGHT_WINDOW_MS: i32 = 3000;

#[derive(Default)]
struct Input {
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
    /// Последнее нажатие, означающее «ухожу из окна»: (время, что нажато).
    leave: Option<(u32, &'static str)>,
    /// Последнее нажатие, которым можно выбрать другое окно (Tab с Alt/Win, Win).
    pick: Option<u32>,
    /// Когда последний раз отпустили Alt (выбор окна в Alt+Tab происходит в этот момент).
    alt_up: Option<u32>,
    /// Последние клики: (время, окно под мышью).
    clicks: Vec<(u32, HWND)>,
}

struct Lock {
    info: WinInfo,
    since: u32,
    /// Спрятана без сворачивания (мягкий режим, вышла в окно или не сворачивается вовсе).
    soft: bool,
    pops: Vec<u32>,
}

pub struct State {
    own_pid: u32,
    pub cfg: Config,
    input: Input,
    last_fg: HWND,
    game: Option<WinInfo>,
    locked: HashMap<HWND, Lock>,
    busy: HashSet<HWND>,
    thief_hits: HashMap<String, Vec<u32>>,
    thief_pause: HashMap<String, u32>,
    pub paused: bool,
    bogus_logged: Option<u32>,
    bogus_skipped: u32,
    poll_n: u32,
}

pub static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn init(cfg: Config) {
    let own_pid = std::process::id();
    *STATE.lock().unwrap() = Some(State {
        own_pid,
        cfg,
        input: Input::default(),
        last_fg: 0,
        game: None,
        locked: HashMap::new(),
        busy: HashSet::new(),
        thief_hits: HashMap::new(),
        thief_pause: HashMap::new(),
        paused: false,
        bogus_logged: None,
        bogus_skipped: 0,
        poll_n: 0,
    });
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    let mut g = STATE.lock().unwrap();
    g.as_mut().map(f)
}

// ---------------- Клавиатура и мышь (Raw Input, как в NoFocusSteal) ----------------

pub fn on_raw_input(lparam: isize, time: u32) {
    let mut buf = [0u8; 64];
    let mut size = buf.len() as u32;
    let header = 24u32; // размер заголовка в 64-битной Windows
    let got = unsafe {
        GetRawInputData(lparam as HRAWINPUT, RID_INPUT, buf.as_mut_ptr() as *mut _, &mut size, header)
    };
    if got == u32::MAX || got < header + 8 {
        return;
    }
    let kind = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let device = isize::from_le_bytes(buf[8..16].try_into().unwrap());
    // Нажатия, созданные программами (а не реальной клавиатурой/мышью), не считаем — так нельзя обмануть.
    if device == 0 {
        return;
    }
    let d = header as usize;
    if kind == 1 {
        // клавиатура
        let flags = u16::from_le_bytes([buf[d + 2], buf[d + 3]]);
        let vk = u16::from_le_bytes([buf[d + 6], buf[d + 7]]);
        let down = flags & 1 == 0;
        with(|s| on_key(&mut s.input, vk, down, time));
    } else if kind == 0 {
        // мышь
        let bf = u16::from_le_bytes([buf[d + 4], buf[d + 5]]);
        const ANY_DOWN: u16 = 0x0001 | 0x0004 | 0x0010 | 0x0040 | 0x0100;
        if bf & ANY_DOWN != 0 {
            let root = unsafe {
                let mut pt = std::mem::zeroed();
                if GetCursorPos(&mut pt) != 0 {
                    let under = WindowFromPoint(pt);
                    if under != 0 { GetAncestor(under, GA_ROOTOWNER) } else { 0 }
                } else {
                    0
                }
            };
            with(|s| {
                let c = &mut s.input.clicks;
                c.push((time, root));
                if c.len() > 16 {
                    c.remove(0);
                }
            });
        }
    }
}

fn on_key(i: &mut Input, vk: u16, down: bool, t: u32) {
    // Отпускание клавиши может потеряться (например, на экране Ctrl+Alt+Del) — сверяемся с реальным состоянием.
    if i.ctrl && !win::key_down_now(0x11) { i.ctrl = false; }
    if i.shift && !win::key_down_now(0x10) { i.shift = false; }
    if i.alt && !win::key_down_now(0x12) { i.alt = false; }
    if i.win && !win::key_down_now(0x5B) && !win::key_down_now(0x5C) { i.win = false; }
    match vk {
        0x10 | 0xA0 | 0xA1 => i.shift = down,
        0x11 | 0xA2 | 0xA3 => i.ctrl = down,
        0x12 | 0xA4 | 0xA5 => {
            if i.alt && !down {
                i.alt_up = Some(t);
            }
            i.alt = down;
        }
        0x5B | 0x5C => {
            i.win = down;
            i.leave = Some((t, "Win"));
            i.pick = Some(t);
        }
        _ if !down => {}
        0x09 if i.alt => {
            i.leave = Some((t, "Alt+Tab"));
            i.pick = Some(t);
        }
        0x09 if i.win => {
            i.leave = Some((t, "Win+Tab"));
            i.pick = Some(t);
        }
        0x44 if i.win => i.leave = Some((t, "Win+D")),
        0x1B if i.ctrl && i.shift => i.leave = Some((t, "Ctrl+Shift+Esc")),
        0x2E if i.ctrl && i.alt => i.leave = Some((t, "Ctrl+Alt+Del")),
        0x1B if i.alt => i.leave = Some((t, "Alt+Esc")),
        _ => {}
    }
}

fn recent(t_event: u32, t_input: u32) -> bool {
    let d = elapsed(t_event.wrapping_add(SLACK_MS as u32), t_input);
    d >= 0 && d <= INTENT_MS + SLACK_MS
}

/// Пользователь только что ушёл из игры: нажал сочетание или кликнул по чему-то другому.
fn leave_intent(s: &State, t: u32, g: &WinInfo) -> Option<&'static str> {
    if let Some((kt, name)) = s.input.leave {
        if recent(t, kt) {
            return Some(name);
        }
    }
    for &(ct, root) in s.input.clicks.iter().rev() {
        if recent(t, ct) && root != 0 && root != g.hwnd && root != g.root {
            return Some(tr("click"));
        }
    }
    None
}

/// Пользователь сам выбрал свёрнутую игру: Alt+Tab / Win+Tab на неё или клик по её кнопке на панели задач.
/// Клики и нажатия в других программах не считаются — иначе игра, вылезшая сама, пока ты работаешь в Chrome, осталась бы.
fn restore_intent(s: &State, t: u32, since: u32, target: HWND, prev_is_switcher: bool) -> bool {
    let after = |x: u32| elapsed(x, since) > RESTORE_GRACE_MS && recent(t, x);
    if let Some(p) = s.input.pick {
        if after(p) {
            let alt_just_released = s.input.alt_up.map_or(false, |u| {
                let d = elapsed(t.wrapping_add(SLACK_MS as u32), u);
                d >= 0 && d <= 400
            });
            if prev_is_switcher || alt_just_released || s.input.alt {
                return true;
            }
        }
    }
    s.input.clicks.iter().any(|&(ct, root)| after(ct) && (root == target || win::is_taskbar_like(root)))
}

// ---------------- Действия ----------------

enum Action {
    None,
    /// Спрятать: свернуть, а если нельзя — убрать под окна и забрать фокус (окно, куда отдать фокус).
    Minimize(WinInfo, bool, HWND),
    Refocus(HWND),
}

fn run(a: Action) {
    match a {
        Action::None => {}
        Action::Refocus(h) => {
            let ok = win::force_foreground(h);
            if !ok {
                log(tr("refocus_fail"));
            }
        }
        Action::Minimize(info, soft, back_to) => {
            let h = info.hwnd;
            if soft {
                win::take_focus_from(h, back_to);
                log(&f("hid_soft", &[&info.exe]));
                with(|s| { s.busy.remove(&h); });
                return;
            }
            std::thread::spawn(move || {
                match win::minimize_ladder(h) {
                    win::Outcome::Minimized(how) => log(&f("minimized", &[&info.exe, tr(how)])),
                    other => {
                        let why = if matches!(other, win::Outcome::LeftFullscreen) {
                            tr("left_full")
                        } else {
                            tr("stuck")
                        };
                        with(|s| {
                            if let Some(l) = s.locked.get_mut(&h) {
                                l.soft = true;
                            }
                        });
                        win::take_focus_from(h, back_to);
                        log(&format!("   {}: {}", info.exe, why));
                    }
                }
                // Следим пару секунд: если игра вылетела после сворачивания — дальше её не сворачиваем.
                let start = tick();
                while elapsed(tick(), start) < 4000 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !win::process_alive(info.pid) {
                        with(|s| {
                            if !s.cfg.soft.contains(&info.exe) {
                                s.cfg.soft.push(info.exe.clone());
                                store::save_config(&s.cfg);
                            }
                        });
                        log(&f("crashed", &[&info.exe]));
                        break;
                    }
                }
                with(|s| { s.busy.remove(&h); });
            });
        }
    }
}

fn lock_and_minimize(s: &mut State, g: &WinInfo, t: u32, back_to: HWND) -> Action {
    let soft = s.cfg.soft.contains(&g.exe);
    s.locked.insert(g.hwnd, Lock { info: g.clone(), since: t, soft, pops: vec![] });
    if s.busy.contains(&g.hwnd) {
        return Action::None;
    }
    s.busy.insert(g.hwnd);
    if win::is_minimized(g.hwnd) {
        s.busy.remove(&g.hwnd);
        log(tr("already_min"));
        return Action::None;
    }
    Action::Minimize(g.clone(), soft, back_to)
}

/// Свёрнутая игра вылезла без участия пользователя — кладём обратно.
fn push_down_again(s: &mut State, h: HWND, t: u32, why: &str, back_to: HWND) -> Action {
    let (info, soft) = {
        let l = match s.locked.get_mut(&h) {
            Some(l) => l,
            None => return Action::None,
        };
        l.pops.retain(|&p| elapsed(t, p) <= FIGHT_WINDOW_MS);
        l.pops.push(t);
        if !l.soft && l.pops.len() >= FIGHT_LIMIT {
            l.soft = true;
            log(&f("fights", &[&l.info.exe]));
        }
        (l.info.clone(), l.soft)
    };
    log(&format!("{}: {}", why, info.describe()));
    if s.busy.contains(&h) {
        return Action::None;
    }
    s.busy.insert(h);
    Action::Minimize(info, soft, back_to)
}

// ---------------- События ----------------

pub fn on_foreground(hwnd: HWND, t: u32) {
    crate::drain_raw_input();
    let a = with(|s| decide(s, hwnd, t)).unwrap_or(Action::None);
    run(a);
}

fn decide(s: &mut State, hwnd: HWND, t: u32) -> Action {
    if hwnd == 0 || hwnd == s.last_fg {
        return Action::None;
    }
    let prev = s.last_fg;
    s.last_fg = hwnd;
    let n = win::capture(hwnd);
    if n.pid == s.own_pid {
        return Action::None;
    }

    // 1. Вылезла свёрнутая нами игра?
    let key = if s.locked.contains_key(&n.hwnd) { Some(n.hwnd) } else if s.locked.contains_key(&n.root) { Some(n.root) } else { None };
    if let Some(k) = key {
        if s.paused {
            s.locked.remove(&k);
            return Action::None;
        }
        let since = s.locked[&k].since;
        let prev_sw = win::is_taskbar_like(prev);
        if restore_intent(s, t, since, k, prev_sw) {
            let l = s.locked.remove(&k).unwrap();
            log(&f("user_opened", &[&l.info.describe()]));
            s.game = Some(n);
            // Игру прятали без сворачивания — помогаем ей стать активной и выйти наверх.
            return if l.soft { Action::Refocus(k) } else { Action::None };
        }
        return push_down_again(s, k, t, tr("popped"), prev);
    }

    // 2. Уходим из игры?
    if let Some(g) = s.game.clone() {
        if prev == g.hwnd && n.hwnd != g.hwnd && win::is_window(g.hwnd) && !s.paused {
            if n.pid == g.pid || n.root == g.hwnd {
                return Action::None; // окно самой игры (лаунчер, диалог)
            }
            if win::is_bogus(&n) {
                // Баг Windows 11: невидимое окно ввода забрало фокус после клика — отдаём его игре.
                let show = s.bogus_logged.map_or(true, |b| elapsed(t, b) >= 60000);
                if show {
                    let more = if s.bogus_skipped > 0 { f("bogus_more", &[&s.bogus_skipped.to_string()]) } else { String::new() };
                    log(&f("bogus", &[&more]));
                    s.bogus_logged = Some(t);
                    s.bogus_skipped = 0;
                } else {
                    s.bogus_skipped += 1;
                }
                s.last_fg = g.hwnd;
                return Action::Refocus(g.hwnd);
            }
            if win::is_overlay(&n) {
                log(&f("overlay", &[&n.exe]));
                return Action::None;
            }
            if let Some(how) = leave_intent(s, t, &g) {
                log(&f("leave", &[how, &g.describe(), &n.describe()]));
                return lock_and_minimize(s, &g, t, n.hwnd);
            }
            if win::is_system_ui(&n) || win::is_hung(g.hwnd) {
                return Action::None;
            }
            // Чужая программа забрала фокус у игры, хотя ты ничего не нажимал.
            if let Some(&until) = s.thief_pause.get(&n.exe) {
                if elapsed(until, t) > 0 {
                    return Action::None;
                }
            }
            let hits = s.thief_hits.entry(n.exe.clone()).or_default();
            hits.retain(|&h| elapsed(t, h) <= FIGHT_WINDOW_MS);
            hits.push(t);
            if hits.len() >= FIGHT_LIMIT {
                hits.clear();
                s.thief_pause.insert(n.exe.clone(), t.wrapping_add(60000));
                log(&f("thief_pause", &[&n.exe]));
                return Action::None;
            }
            log(&f("thief", &[&n.describe()]));
            s.last_fg = g.hwnd;
            return Action::Refocus(g.hwnd);
        }
    }

    // 3. Новая активная игра?
    if win::looks_like_game(&n, &s.cfg.ignore, s.own_pid) {
        let changed = s.game.as_ref().map_or(true, |g| g.hwnd != n.hwnd);
        if changed {
            log(&f("game_seen", &[&n.describe(), if s.cfg.soft.contains(&n.exe) { tr("soft_mark") } else { "" }]));
        }
        s.game = Some(n);
    }
    Action::None
}

/// Windows сообщила, что окно развернулось из свёрнутого состояния.
pub fn on_restored(h: HWND, t: u32) {
    crate::drain_raw_input();
    let a = with(|s| {
        if s.paused || !s.locked.contains_key(&h) || s.locked[&h].soft || s.busy.contains(&h) {
            return Action::None;
        }
        let since = s.locked[&h].since;
        let prev_sw = win::is_taskbar_like(s.last_fg);
        if restore_intent(s, t, since, h, prev_sw) {
            if let Some(l) = s.locked.remove(&h) {
                log(&f("user_restored", &[&l.info.describe()]));
            }
            return Action::None;
        }
        push_down_again(s, h, t, tr("restored_self"), 0)
    })
    .unwrap_or(Action::None);
    run(a);
}

/// Ctrl+Alt+Del, экран блокировки, окно UAC — Windows переключилась на отдельный экран.
pub fn on_desktop_switch(t: u32) {
    let a = with(|s| {
        if s.paused {
            return Action::None;
        }
        let g = match s.game.clone() {
            Some(g) => g,
            None => return Action::None,
        };
        if s.last_fg != g.hwnd || s.locked.contains_key(&g.hwnd) || !win::is_window(g.hwnd) {
            return Action::None;
        }
        log(&f("desktop_switch", &[&g.describe()]));
        lock_and_minimize(s, &g, t, 0)
    })
    .unwrap_or(Action::None);
    run(a);
}

/// Вызывается каждые 15 мс: ловим смену окна, о которой Windows не сообщила, и сторожим свёрнутые игры.
pub fn poll() {
    let fg = win::foreground();
    let last = with(|s| s.last_fg).unwrap_or(0);
    if fg != 0 && fg != last {
        on_foreground(fg, tick());
    }
    let t = tick();
    let acts: Vec<Action> = with(|s| {
        s.poll_n = s.poll_n.wrapping_add(1);
        let mut out = vec![];
        if s.poll_n % 7 != 0 {
            return out;
        }
        s.locked.retain(|h, _| win::is_window(*h));
        if s.paused {
            s.locked.clear();
            return out;
        }
        let ids: Vec<HWND> = s.locked.keys().copied().collect();
        for h in ids {
            let (soft, since) = { let l = &s.locked[&h]; (l.soft, l.since) };
            if soft || s.busy.contains(&h) || win::is_minimized(h) || !win::is_visible(h) {
                continue;
            }
            if restore_intent(s, t, since, h, false) {
                if let Some(l) = s.locked.remove(&h) {
                    log(&f("user_restored", &[&l.info.describe()]));
                }
                continue;
            }
            out.push(push_down_again(s, h, t, tr("restored_self"), 0));
        }
        if s.poll_n % 140 == 0 && win::restore_focus_protection() {
            log(tr("protection"));
        }
        out
    })
    .unwrap_or_default();
    for a in acts {
        run(a);
    }
}

pub fn toggle_log() {
    with(|s| {
        s.cfg.log = !s.cfg.log;
        if s.cfg.log {
            store::set_log_enabled(true);
            log(tr("log_on"));
        } else {
            log(tr("log_off"));
            store::set_log_enabled(false);
        }
        store::save_config(&s.cfg);
    });
}

pub fn toggle_pause() -> bool {
    with(|s| {
        s.paused = !s.paused;
        s.locked.clear();
        log(tr(if s.paused { "pause_on" } else { "pause_off" }));
        s.paused
    })
    .unwrap_or(false)
}
