# EXP11 - Notification-area (system tray) icon tinted by the verdict

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Add a Windows tray icon - a solid square in the current verdict colour
(green/amber/red) - that updates whenever the verdict changes, with a tooltip
naming the state and location. Closing the window hides it to the tray instead
of quitting; right-clicking the icon offers Show/Hide and Quit. Hand-rolled
Win32, app-only.

## Hypothesis
gpui-ce 0.2.2 exposes no tray API, but the app can host a notification-area icon
on a dedicated Win32 thread (`Shell_NotifyIconW` + a hidden message window) and
repaint it to the verdict colour by rebuilding a 32-bit `HICON`
(`CreateDIBSection` + `CreateIconIndirect`) on each verdict change, **without any
crate beyond the already-present `windows` (and `raw-window-handle`)**, without
touching gpui's UI thread, and without regressing the pixel E2E. gpui's
`on_window_should_close` can turn the titlebar X / Alt+F4 into `SW_HIDE` (process
and icon stay alive), and a Win32 popup menu on the tray thread can toggle
visibility and perform a real quit by flagging and replaying `WM_CLOSE`, so the
app exits only via the tray's Quit (or Ctrl+Q).

**Falsified if:** adding `windows` as a direct dep fails to link under MinGW;
`Shell_NotifyIconW` won't add/modify an icon from a background thread; the
verdict->icon-colour wiring can't be asserted in the E2E; closing the window
still exits the app or the tray Quit fails to exit it; or the closed-state pixel
counts change (`closed=2043 open=790 panel=0`).

## Methodology & blast radius
- Method (app-only):
  1. New `app/src/tray.rs`.
     - Pure `icon_bgra(color, size) -> Vec<u8>`: solid verdict-coloured square +
       1px white border, in BGRA (top-down DIB layout). The only colour logic;
       fully unit-testable.
     - `start(window_hwnd)`: one dedicated thread registers a window class,
       creates a **hidden** top-level window, `NIM_ADD`s the icon
       (`uID=1`, `uCallbackMessage=WM_APP+1`, `NIF_ICON|NIF_MESSAGE|NIF_TIP|NIF_SHOWTIP`,
       then `NIM_SETVERSION` = 4) and pumps `GetMessageW`.
     - `set_verdict(verdict, location)`: map verdict->theme colour + tooltip,
       store in a global, and post a refresh message to the tray thread (which
       rebuilds the HICON and `NIM_MODIFY`s it). Pending value is picked up at
       thread start, so the call order against `start` does not matter.
     - `set_loading(location)`: muted grey while a new location is fetching.
     - Left-click -> `show_main()` (`SW_RESTORE`+`SetForegroundWindow`) on the
       gpui HWND (from the raw-window-handle handed in by `main.rs`);
       `WM_DESTROY` -> `NIM_DELETE`.
     - Right-click -> Win32 popup menu (`CreatePopupMenu`/`AppendMenuW`/
       `TrackPopupMenu` with `TPM_RETURNCMD`): "Show"/"Hide" (label from
       `IsWindowVisible`) + separator + "Quit"; a real quit flags
       `QUIT_REQUESTED` and replays `WM_CLOSE` through `on_window_should_close`.
     - `should_close()`: hides (`SW_HIDE`) and returns `false` unless a quit was
       requested or the tray never became ready. Pure `close_action` /
       `toggle_label` are unit-tested.
     - Test seams `AIRQLALITY_TRAY_DUMP=<path>` (writes
       `{"color":"#3fb950","tip":"...","ok":true}` on each update) and
       `AIRQLALITY_TRAY_QUIT_MS=<n>` (replays the menu's quit path).
  2. `app/src/main.rs`: read the gpui window's native HWND (`gpui::Window:
     HasWindowHandle`), `tray::start(hwnd)`, and register
     `window.on_window_should_close(cx, |_,_| tray::should_close())`.
  3. `app/src/ui.rs`: call `tray::set_verdict` after every `evaluate` in
     `set_series` and `tick`; `tray::set_loading` on location switch. The ticker
     / 30-min refresh keep the hidden icon current.
  4. `app/Cargo.toml`: `windows` (Win32_Foundation, Win32_UI_Shell,
     Win32_UI_WindowsAndMessaging, Win32_Graphics_Gdi,
     Win32_System_LibraryLoader) + `raw-window-handle`. Both already in the
     lockfile via `gpui_ce_windows` -> no new download.
- Verify: `cargo test -p airqlality-core` (21/21 unchanged); `cargo test -p
  airqlality` (7/7 bin incl. 5 `tray`, + e2e closed counts unchanged).
  New E2E: mock+frozen -> dump `KeepClosed`/`CLOSED`; `AIRQLALITY_MOCK_NOW=36` ->
  dump `OPEN`; `app/tests/tray.ps1 -Mode hide` posts WM_CLOSE and asserts
  `ALIVE=1 VISIBLE=0 DUMP=1`; `-Mode quit` asserts `EXITED code=0`.
- Evidence into `experiments/EXP11/`: taskbar-strip captures
  (`EXP11-tray-{closed,open}-zoom.png`, `EXP11-tray-after-close-zoom.png`), the
  open menu (`EXP11-tray-menu.png`), and the dump JSONs.
- May touch: `app/src/{main.rs,ui.rs,tray.rs}`, `app/Cargo.toml`,
  `app/tests/{e2e_widget.rs,capture.ps1,tray.ps1}`, `experiments/EXP11/**`,
  `experiments-overview.md`, branch `EXP11` (from `d4d7e1c`).
- Off-limits: `core/**`, `UI.md`, `AGENTS.md`, `experiments/EXP01..10/**`
  (frozen evidence), declaring a final status.
- Machine-level blast radius: one extra OS thread + one hidden window + one
  ~16px icon; the window can now be hidden with no visible frame; one popup menu
  on right-click; no new crate download.

## Open questions (known unknowns)
- [x] Colour mapping - 3-state verdict -> `theme::{OPEN,VENT,CLOSED}`, matching
      the header chip. (assumption: consistent + already contrast-checked EXP10)
- [x] Which hour - the current "now" `Decision.headline`. (assumption: the tray
      answers "can I open right now?", like the header)
- [x] Shape - solid square + 1px white border for light/dark taskbar visibility.
- [x] Click - left-click shows/activates the widget via its raw HWND;
      right-click opens the Show/Hide + Quit menu (`TrackPopupMenu`, owner = the
      hidden tray window).
- [x] Tooltip - `AirQlality - <location> - <VERDICT>`.
- [x] Test proof - `icon_bgra` unit tests + the `AIRQLALITY_TRAY_DUMP` seam for
      the verdict->colour wiring; tray screenshot best-effort. (a fixed-GUID
      `Shell_NotifyIconGetRect` presence probe is a stretch; the icon may sit in
      the overflow)
- [x] Window close - **assumption revised mid-experiment**: close (X / Alt+F4)
      now hides to the tray via `on_window_should_close` returning `false`;
      Quit is the tray menu (Ctrl+Q kept). If the tray never became ready, close
      quits normally rather than hiding with no way back.
- [x] Stale icons - rely on the hidden window's destruction at process exit, and
      `NIM_DELETE` on `WM_DESTROY` for graceful exits.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** gpui-ce 0.2.2's lack of a tray API was no obstacle: the
widget now has a notification-area icon - a solid square in the current verdict
colour - that repaints as the data changes, plus tray-app window lifecycle
(close hides, right-click Show/Hide + Quit), all hand-rolled Win32 on one extra
thread and **no new crate** (`windows`/`raw-window-handle` were already in the
graph).

- **Icon.** A dedicated thread owns a hidden top-level window + one
  `Shell_NotifyIconW` icon (version 4). Verdict changes rebuild a 32-bit `HICON`
  (`CreateDIBSection` + `CreateIconIndirect`, pure `icon_bgra` square + 1px white
  border) and `NIM_MODIFY`. Colour = `theme::{OPEN,VENT,CLOSED}` for the current
  "now" headline; tooltip `AirQlality - <location> - <VERDICT>`; grey while a new
  location fetches.
- **Lifecycle.** `Window::on_window_should_close` returns `false` on the titlebar
  X / Alt+F4 and hides the window (`SW_HIDE`) - the process, the ticker/refresh
  and the icon stay alive. Left-click shows/activates. Right-click opens a Win32
  popup menu (Show/Hide + Quit). Quit is the only real exit
  (`QUIT_REQUESTED` + replayed `WM_CLOSE` -> gpui closes the last window,
  `exit 0`); Ctrl+Q is kept. If the tray never became ready, close quits normally.
- **Assumptions resolved.** "Windows 11 hides new icons in the overflow" was
  **wrong on this machine** - the icon showed directly in the taskbar (324 px of
  exact colour; ~18 px at x 2209..2226 of 2560). The window-close assumption
  *was* revised mid-experiment (see Open questions) and is now the behaviour.
- **Tests.** core 21/21; app 7/7 (5 `tray`: colour mapping, `icon_bgra`,
  `close_action`, `toggle_label`); e2e 12/12 (4 tray: CLOSED red / OPEN green
  dumps via `AIRQLALITY_TRAY_DUMP` + `AIRQLALITY_MOCK_NOW`, hide-on-close
  `ALIVE=1 VISIBLE=0 DUMP=1`, clean quit `EXITED code=0`). Closed-state pixel
  asserts unchanged (`closed=2043 open=790 panel=0`). Evidence:
  `EXP11-{tray-closed,tray-open,tray-after-close}-zoom.png`,
  `EXP11-tray-menu.png`, `EXP11-widget-{closed,open}.png`, `EXP11-tray-*.json`.
- **Not proven (out of scope):** the popup menu is verified by a synthetic
  right-click + captured menu, not a committed E2E (Windows input injection is
  unreliable, like EXP10's `Ctrl+Q`); no per-monitor-DPI icon (uses
  `SM_CXSMICON`); no notification-area "always visible" pin; no minimize-to-tray;
  no window/taskbar icon (tray-only, as requested).

**Recommended next:** disk cache so a restart shows the last good series offline,
then always-on-top; then the taskbar/window icon and per-monitor DPI if wanted.

## Learnings
- **Native HWND from gpui:** `gpui::Window` has an inherent `window_handle()`
  (returns `AnyWindowHandle`) that shadows `HasWindowHandle::window_handle()`;
  call the trait method explicitly and match `RawWindowHandle::Win32(w)` ->
  `w.hwnd.get()`. `raw-window-handle` 0.6 is already in the graph.
- **Tray recipe (Win32, one thread):** hidden top-level window + `Shell_NotifyIconW`
  with `NIM_SETVERSION` = 4; a 32bpp top-down DIB -> `CreateIconIndirect` for the
  `HICON`; repaint by `PostMessage` from the UI thread -> `NIM_MODIFY`. The icon
  appears directly in the taskbar (not the overflow) on this machine.
- **gpui close veto + quit:** `Window::on_window_should_close(cx, |w,cx| -> bool)`
  returning `false` makes the Windows backend swallow `WM_CLOSE`; `QuitMode`
  defaults to `LastWindowClosed`, so a never-closed window keeps the app alive.
  `AsyncApp` is not `Send`, so quit from another thread = set a flag +
  `PostMessage(MAIN_HWND, WM_CLOSE)` (the callback then returns `true`) - no
  polling loop.
- **Popup menu:** `CreatePopupMenu` + `AppendMenuW` + `TrackPopupMenu`
  (`TPM_RETURNCMD|TPM_RIGHTBUTTON|TPM_NONOTIFY`), owner = the hidden window;
  `SetForegroundWindow(owner)` before and `PostMessage(owner, WM_NULL)` after.
- **Test seams:** `AIRQLALITY_TRAY_DUMP` (icon colour/ok), `AIRQLALITY_MOCK_NOW`
  (pin the mock hour), `AIRQLALITY_TRAY_QUIT_MS` (replay the quit path);
  `app/tests/tray.ps1` posts `WM_CLOSE` for hide/quit.
- **Numbers:** PATH += MinGW; core 21/21, app 7/7, e2e 12/12; closed counts
  `closed=2043 open=790 panel=0` unchanged. `windows` features: Win32_Foundation,
  Graphics_Gdi, System_LibraryLoader, UI_Shell, UI_WindowsAndMessaging (already
  transitive). Benign MinGW link warning `.rsrc merge failure`.
