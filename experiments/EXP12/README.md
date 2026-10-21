# EXP12 - Run at startup, toggled from the tray menu

**Status:** `success` (user-declared, 2026-10-05).

## Summary
Let the widget register itself to launch at login and add a **"Run at startup"**
item to the tray right-click menu whose native checkmark reflects whether it is
registered. Opt-in, per-user (`HKCU\...\Run`), no admin, no new crate. A login
launch starts hidden in the tray.

## Hypothesis
The app can implement run-at-startup with the already-present `windows` crate by
adding only the `Win32_System_Registry` feature (same crate, no download):
read/write one `REG_SZ` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. A Win32 popup menu renders
a native checkmark via `MF_CHECKED`, so the tray menu shows "Run at startup"
checked/unchecked from a live query and a click toggles registration. Writing
`--startup` into the command and opening with
`WindowOptions { show: false, focus: false }` makes a login launch start hidden
in the tray.

**Falsified if:** the registry API fails to link under MinGW; `MF_CHECKED` does
not render; a toggle cannot be asserted black-box; hidden startup leaves no
window to restore; or the closed-state pixel counts change
(`closed=2043 open=790 panel=0`).

## Methodology & blast radius
- Method (app-only):
  1. New `app/src/startup.rs`.
     - Pure, unit-tested: `command_for(exe)` (normalize a leading `\\?\`,
       produce `"<exe>" --startup`), `is_enabled(stored: Option<&str>, exe)`
       (trim + case-insensitive compare against the desired command), and
       `menu_flags(registered)` -> `MF_STRING | MF_CHECKED?`.
     - OS layer (runs on the tray thread): `is_registered()`
       (`RegOpenKeyExW`/`RegQueryValueExW`), `toggle()`/`set_registered(bool)`
       (`RegSetValueExW`/`RegDeleteValueW`), HKCU only, `REG_SZ`, value name
       `AirQlality` (overridable via `AIRQLALITY_STARTUP_NAME`).
  2. `app/src/tray.rs`: add `MENU_STARTUP`; in `show_menu` query
     `is_registered()` and append `"Run at startup"` with
     `MF_STRING | MF_CHECKED` when registered; the command -> `startup::toggle()`.
  3. `app/src/main.rs`: detect `--startup`; when present open with
     `show: false, focus: false` and skip `cx.activate(true)`. Tray left-click /
     menu "Show" restores the window as today.
  4. `app/Cargo.toml`: add `"Win32_System_Registry"` to the `windows` features
     (already in the graph -> no new download).
  5. Test seams (EXP11 style): `AIRQLALITY_STARTUP_NAME` (value name),
     `AIRQLALITY_STARTUP_TOGGLE=1` (replays the menu's toggle once),
     `AIRQLALITY_STARTUP_DUMP=<path>` (writes
     `{"registered":bool,"command":"..."}`).
- Verify: `cargo test -p airqlality-core` (21/21 unchanged); new `startup` unit
  tests; a new `app/tests/startup.ps1` + E2E cases in `e2e_widget.rs` using the
  value name `AirQlalityE2E`: toggle-on writes `"<exe>" --startup` (asserted via
  `Get-ItemProperty`), toggle-off removes it, a foreign value reads as
  unchecked, and a `--startup` launch reports `ALIVE=1 VISIBLE=0`, all with the
  test value cleaned up and the real `AirQlality` entry untouched. Closed-state
  pixel counts unchanged.
- Evidence into `experiments/EXP12/`: the tray menu with the item checked and
  unchecked (`EXP12-tray-menu-{on,off}-zoom.png`, via a real right-click -
  `capture-menu.ps1`) and the startup dump JSONs
  (`EXP12-startup-{on,off}.json`).
- May touch: `app/src/{startup.rs,tray.rs,main.rs}`, `app/Cargo.toml`,
  `app/tests/{e2e_widget.rs,startup.ps1}`, `experiments/EXP12/**`,
  `experiments-overview.md`, branch `EXP12` (from `b41f495`).
- Off-limits: `core/**`, `UI.md`, `AGENTS.md`, `experiments/EXP01..11/**`
  (frozen evidence), declaring a final status.
- Machine-level blast radius: writes/deletes exactly one HKCU `Run` value
  (`AirQlality`) when enabled, so the app may start at login; the E2E uses a
  separate test-named value and deletes it. No admin rights, no new crate
  download, no new OS thread.

## Open questions (known unknowns)
- [x] Default state - **user decision: opt-in**, the app never auto-registers;
      the menu item starts unchecked.
- [x] Startup launch - **user decision: hidden in the tray** (`--startup` +
      `WindowOptions { show: false, focus: false }`).
- [x] Checkmark semantics - checked iff the stored value matches the current
      exe's command; a moved/stale path shows unchecked.
- [x] Menu label - `"Run at startup"`.
- [x] E2E isolation - `AIRQLALITY_STARTUP_NAME=AirQlalityE2E` + delete in the
      test; never touch the real `AirQlality` entry.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-05.**

**Hypothesis upheld.** Run-at-startup is done with the already-present `windows`
crate (adding only the `Win32_System_Registry` feature - **no new crate**, no
download), as one per-user `REG_SZ` value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`; the tray menu's native
checkmark reflects it, and a login launch opens hidden in the tray.

- **Registration.** Toggling writes `"<current exe>" --startup` (or deletes the
  value). The value name defaults to `AirQlality`; a leading `\\?\` verbatim
  prefix is stripped and the command is always quoted. "Registered" means the
  stored value equals the current exe's command (case-insensitive), so a
  moved/stale entry reads as unchecked.
- **Menu.** The tray right-click menu gained a **"Run at startup"** item between
  Show/Hide and Quit, appended with `MF_STRING | MF_CHECKED` when registered, so
  Windows draws the check mark. Clicking it runs `startup::toggle()`. Opt-in:
  nothing registers unless the user clicks it.
- **Hidden start.** `main` detects `--startup` and opens with
  `WindowOptions { show: false, focus: false }`, skipping `cx.activate(true)`;
  the window exists and is restorable by tray left-click / "Show".
- **Tests.** core 21/21; app 12/12 (5 new `startup`: command quoting, verbatim
  stripping, match semantics, arg detection; + 1 `tray` checkmark flag);
  e2e 15/15 (3 new: toggle-on writes the value, toggle-off removes it, and a
  `--startup` launch is `ALIVE=1 VISIBLE=0`). Closed-state pixel counts
  unchanged (`closed=2043 open=790 panel=0`). The E2E isolates itself with the
  value name `AirQlalityE2E` and deletes it; the real `AirQlality` entry was
  never written.
- **Evidence.** `EXP12-tray-menu-on-zoom.png` (check mark) /
  `EXP12-tray-menu-off-zoom.png` (no check mark), captured from a real
  right-click (`capture-menu.ps1`); `EXP12-startup-{on,off}.json` (the app's own
  view); `app/tests/startup.ps1`.
- **Not proven (out of scope):** the toggle's effect on an actual reboot/logon
  (verified via the registry, not by logging out); `HKLM`/all-users scope; a
  "start minimized vs hidden" preference; per-monitor DPI of the menu; and the
  menu check mark is captured manually, not asserted in a committed E2E (Windows
  popups need the foreground lock, so a backend seam cannot open one).

**Recommended next:** disk cache so a restart shows the last good series
offline, then always-on-top; the taskbar/window icon if wanted.

## Learnings
- **Per-user startup (no admin):** `windows`' `Win32_System_Registry` feature,
  `HKCU\...\Run`, one `REG_SZ` value. In windows 0.62 `RegOpenKeyExW`'s
  `ulOptions` and `RegSetValueExW`'s `reserved` are `Option<u32>` (pass
  `Some(0)`); `...W`/`RegCloseKey` return `WIN32_ERROR`, compared with
  `ERROR_SUCCESS`; `REG_SZ` data is UTF-16LE with a NUL terminator.
- **Tray checkmark:** `AppendMenuW(menu, MF_STRING | MF_CHECKED, id, label)` -
  the OS draws the check; no owner-draw.
- **Hidden start:** `WindowOptions { show: false, focus: false }` in gpui-ce
  0.2.2, plus skipping `cx.activate(true)`; the tray can still `SW_RESTORE` it.
- **Popup needs the foreground lock:** a background thread cannot foreground its
  hidden window, so `TrackPopupMenu` from a test seam returns without showing.
  Capture the menu with a *real* right-click at the tray icon (`SetCursorPos` +
  `mouse_event`), see `experiments/EXP12/capture-menu.ps1` (icon ~(2216,1416)
  on this 2560x1440 desktop).
- **Test seams:** `AIRQLALITY_STARTUP_NAME` (Run value name; tests use
  `AirQlalityE2E`), `AIRQLALITY_STARTUP_TOGGLE=1` (replays the menu's toggle),
  `AIRQLALITY_STARTUP_DUMP`; `app/tests/startup.ps1` drives on/off/hidden and
  cleans up.
- **Numbers:** PATH += MinGW; core 21/21, app 12/12, e2e 15/15; closed counts
  unchanged (`closed=2043 open=790 panel=0`). Benign MinGW `.rsrc merge failure`
  warning.

