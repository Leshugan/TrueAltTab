<p align="center">
  <img src="assets/logo.png" width="128" alt="TrueAltTab">
</p>

<h1 align="center">TrueAltTab</h1>

<p align="center">Minimizes games for real and keeps them from restoring themselves.</p>

<p align="center">
  <a href="https://github.com/Leshugan/TrueAltTab/releases/latest"><img src="https://img.shields.io/github/v/release/Leshugan/TrueAltTab?label=&color=2F80ED" alt="release"></a>
  <a href="https://github.com/Leshugan/TrueAltTab/releases"><img src="https://img.shields.io/github/downloads/Leshugan/TrueAltTab/total?color=555" alt="downloads"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4" alt="Windows 10 | 11">
  <img src="https://img.shields.io/badge/Rust-000000?logo=rust" alt="Rust">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-555" alt="MIT"></a>
</p>

<p align="center">
  <a href="https://github.com/Leshugan/TrueAltTab/releases/latest/download/TrueAltTab.exe"><b>⬇ Download TrueAltTab.exe</b></a>
</p>

<p align="center"><b>English</b> · <a href="README.ru.md">Русский</a> · <a href="README.es.md">Español</a></p>

---

## Why

Minimizing a game on Windows can be a real pain. Some games refuse to minimize
at all and stay on top of everything. Others minimize, but pop right back up
a second later as soon as you open a browser or File Explorer. Others drop out
of fullscreen and just hang there as a window.

TrueAltTab brings back the Windows 7 behavior: leave the game — the game is
minimized and stays that way until you open it yourself.

## What it does

- **Minimizes the game** when you leave it: Alt+Tab, Win+Tab, Win (Start menu),
  Win+D, Ctrl+Shift+Esc, Ctrl+Alt+Del, lock screen.
- **Keeps it minimized.** If the game tries to come back by itself, the program
  hides it again right away. Only you can open it: via Alt+Tab, Win+Tab
  or its taskbar button.
- **Handles stubborn games.** If a game won't minimize the normal way, the program
  tries increasingly forceful methods. If the game leaves fullscreen instead of
  minimizing, or won't minimize at all, the program puts it behind other windows
  and takes its focus away.
- **Protects the game from focus stealing.** If another program pops up over
  the game without you pressing anything, focus goes back to the game.
- **Works around the Windows 11 bug** that makes games lose focus on every click.
- **Adapts to each game by itself.** If a game crashes when minimized or keeps
  coming back, the program remembers it and hides it without minimizing from then on.
  No setup needed.

## What it doesn't do

- Doesn't inject into games or modify their files — safe with anti-cheats.
- Doesn't press keys on your behalf.
- Doesn't write to the registry or change anything in Windows. Close the program —
  everything is as it was.
- Doesn't intercept or block your shortcuts. Alt+Tab and the rest work as usual.

## Installation

Download `TrueAltTab.exe`, put it in any folder and run it. An icon appears in the tray.

Tray menu (right click):
- **Pause** — temporarily turn it off.
- **Start with Windows** — autostart via Task Scheduler.
- **Keep a log** — turn logging on or off.
- **Open log**
- **Exit**

The program runs as administrator — without it Windows won't let it manage
games that run as administrator.

## Windows Defender

Windows Defender or SmartScreen may warn you about the program or even delete it.
This is a false alarm. Here is why it happens:

- **The program has no digital signature.** A signing certificate costs money every
  year, and the program is free. SmartScreen greets an unsigned exe from an unknown
  author with a "Windows protected your PC" window.
- **The program behaves a bit like spyware, even though it isn't.** It runs as
  administrator, watches window changes, notices key presses and clicks, and can add
  itself to autostart. Defender doesn't know why — it only sees a set of actions
  that is common in malware.

What proves otherwise:

- The program only remembers **when** keys are pressed, not which keys. It saves
  nothing and sends nothing anywhere — it has no internet code at all.
- It doesn't inject into other programs and doesn't change files or the registry.
- The source code is open: you can read it and build the exe yourself.
- You can check the exe on [VirusTotal](https://www.virustotal.com).

If SmartScreen shows a warning, click **"More info" → "Run anyway"**.
If Defender deleted the file, restore it in "Protection history" and add the
program's folder to the exclusions.

## Language

Russian, English and Spanish. By default it follows Windows. To choose manually,
set `Language = en` (or `ru`, `es`, `auto`) in `TrueAltTab.ini`.

## Files

- `TrueAltTab.ini` next to the exe — language, logging, the list of programs
  to never touch, and the list of games to hide without minimizing
  (the program maintains that one itself).
- `TrueAltTab.log` on the desktop — a log of which game it was, what it did
  and how it was minimized. Can be turned off in the menu. If something goes
  wrong, turn the log on and attach it to your issue.

## Requirements

Windows 10 / 11, 64-bit.

## Building from source

You need [Rust](https://rustup.rs) and mingw-w64:

```
rustup target add x86_64-pc-windows-gnu
cargo build --release
```

Output: `target/x86_64-pc-windows-gnu/release/TrueAltTab.exe`.
Or just fork the repository — GitHub Actions will build the exe for you.

## Credits

The way of tracking window focus without touching other programs was inspired by
[NoFocusSteal](https://github.com/BoazCohenJ/NoFocusSteal) (MIT).

## License

[MIT](LICENSE)
