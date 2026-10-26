//! Windows notification-area (system tray) icon, tinted by the verdict (EXP11).
//!
//! gpui-ce has no tray API, so this is hand-rolled Win32 on a **dedicated
//! thread**: a hidden top-level window owns one `Shell_NotifyIconW` icon, whose
//! `HICON` is rebuilt (a 32-bit DIB -> `CreateIconIndirect`) whenever the verdict
//! changes. The UI thread only stores the desired colour/tooltip and posts a
//! refresh message, so it never blocks on the shell.
//!
//! The same thread owns the window lifecycle: left-click shows the widget,
//! right-click pops up a Show/Hide + Refresh + API keys + Run at startup + Quit
//! menu, and closing the window hides to the tray (via gpui's
//! `on_window_should_close`) instead of quitting. Menu items that must act on
//! the widget reach it through [`crate::commands`], not through a window
//! message (EXP22, EXP24).
//!
//! All colour logic lives in the pure [`icon_bgra`], and the close decision in
//! [`close_action`], so both are unit-testable without touching the OS.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Mutex;
use std::thread;

use airqlality_core::Verdict;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_INFO,
    NIIF_RESPECT_QUIET_TIME, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, IsWindow, IsWindowVisible,
    PostMessageW, PostQuitMessage, RegisterClassW, SetForegroundWindow, ShowWindow, TrackPopupMenu,
    TranslateMessage, HICON, HMENU, MENU_ITEM_FLAGS, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR,
    MF_STRING, MSG, SM_CXSMICON, SM_CYSMICON, SW_HIDE, SW_RESTORE, TPM_NONOTIFY, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_NULL,
    WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

use crate::commands::{self, Command};
use crate::notify;
use crate::persist;
use crate::startup;
use crate::theme;

/// The icon's 1px border: near-white reads against a dark taskbar, while the
/// bright coloured fill keeps the square visible on a light one.
const BORDER: u32 = 0xffffff;

/// `Shell_NotifyIcon`'s callback message; LOWORD(lParam) is the event.
const WM_TRAY: u32 = WM_APP + 1;
/// Sent from the UI thread to ask the tray thread to repaint the icon.
const WM_TRAY_REFRESH: u32 = WM_APP + 2;
/// The (hwnd, uID) pair that names our icon in the notification area.
const TRAY_ID: u32 = 1;
/// Popup-menu command ids.
const MENU_TOGGLE: u32 = 1;
const MENU_QUIT: u32 = 2;
const MENU_STARTUP: u32 = 3;
const MENU_REFRESH: u32 = 4;
const MENU_KEYS: u32 = 5;
/// The "Notify me on" submenu's three checks (EXP28).
const MENU_NOTIFY_CLOSED: u32 = 6;
const MENU_NOTIFY_OPEN: u32 = 7;
const MENU_NOTIFY_VENT: u32 = 8;

/// The submenu's label.
pub const NOTIFY_MENU: &str = "Notify me on";

/// The hidden tray window's handle (0 until the tray thread has created it).
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
/// The app's top-level window, brought forward on a tray click (0 if unknown).
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
/// Set by the tray's Quit so the next `WM_CLOSE` becomes a real quit.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
/// Whether the shell accepted the icon (so hiding always has a way back).
static TRAY_READY: AtomicBool = AtomicBool::new(false);

struct State {
    color: u32,
    tip: String,
    /// Which verdicts may notify (EXP28), shared by the menu's checks and the
    /// widget's planner. `None` until first read, so a warning planned before
    /// the tray thread exists still honours the saved choice.
    targets: Option<notify::Targets>,
    /// A balloon waiting for the tray icon to exist. Sent by
    /// `add_icon`/`refresh_icon`, then cleared (EXP17).
    pending: Option<Pending>,
}

/// A balloon in flight (EXP17, EXP30). The tray transports it; the widget
/// decided it, including when (`at`) and why (`kind`).
struct Pending {
    title: String,
    body: String,
    /// The warned run's first hour, as an absolute unix-hour bucket.
    hour: i64,
    verdict: Verdict,
    /// The evaluation instant, in the location's wall clock, to the minute.
    at: String,
    /// `ahead` / `now` / `late`.
    kind: &'static str,
}

/// The desired icon, shared between the UI thread (writer) and tray thread.
static STATE: Mutex<State> = Mutex::new(State {
    color: theme::MUTED,
    tip: String::new(),
    targets: None,
    pending: None,
});

impl State {
    /// The chosen notification targets, loaded once from disk (EXP28). The
    /// tray menu updates this copy, so a toggle takes effect immediately.
    fn targets(&mut self) -> notify::Targets {
        *self.targets.get_or_insert_with(persist::load_notify)
    }
}

/// The choices the widget's planner must honour, read from the same state the
/// menu writes (EXP28, EXP30).
pub fn notify_targets() -> notify::Targets {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    state.targets()
}

/// Map a verdict to the widget's palette (identical to the header chip/legend).
pub fn color_for(verdict: Verdict) -> u32 {
    match verdict {
        Verdict::Open => theme::OPEN,
        Verdict::Vent => theme::VENT,
        Verdict::KeepClosed => theme::CLOSED,
    }
}

/// A `size * size`, 32bpp top-down **BGRA** buffer for the tray icon: a solid
/// `color` square inside a 1px [`BORDER`]. Pure, so the colour contract is
/// testable without Windows.
pub fn icon_bgra(color: u32, size: u32) -> Vec<u8> {
    let size = size.max(1);
    let fill = bgra(color);
    let border = bgra(BORDER);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let edge = x == 0 || y == 0 || x == size - 1 || y == size - 1;
            out.extend_from_slice(if edge { &border } else { &fill });
        }
    }
    out
}

/// 0xRRGGBB -> `[B, G, R, 0xff]`, the byte order a 32bpp DIB expects.
fn bgra(color: u32) -> [u8; 4] {
    [
        (color & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        ((color >> 16) & 0xff) as u8,
        0xff,
    ]
}

/// Start the tray thread. `window_hwnd` is the app's top-level window (0 when it
/// could not be resolved); it is used to show/hide the widget from the tray.
pub fn start(window_hwnd: isize) {
    let _ = thread::Builder::new()
        .name("airqlality-tray".into())
        .spawn(move || unsafe { run(window_hwnd) });
}

/// Point the icon at a verdict: colour + tooltip (EXP11).
///
/// Since EXP30 this is all it does. The balloon is planned from the data's own
/// clock by [`crate::onset::Planner`] and delivered through [`warn`], so a
/// fetch that re-evaluates the same hour can never fire one.
pub fn set_verdict(verdict: Verdict, location: &str) {
    let color = color_for(verdict);
    let tip = format!(
        "AirQlality \u{00b7} {location} \u{00b7} {}",
        verdict.label()
    );
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        // The evaluation clock repaints once a minute (EXP30); only a real
        // change needs a new HICON in the shell.
        if state.color == color && state.tip == tip {
            return;
        }
        state.color = color;
        state.tip = tip;
    }
    nudge();
}

/// Queue a balloon the widget's planner has already decided on (EXP30).
///
/// `hour` is the warned run's first hour, `at` the evaluation instant and
/// `kind` one of `ahead` / `now` / `late`; all three reach the E2E dump, so the
/// timing is asserted at the Win32 boundary, not inferred from the UI.
pub fn warn(verdict: Verdict, title: &str, body: &str, hour: i64, at: &str, kind: &'static str) {
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.pending = Some(Pending {
            title: title.to_string(),
            body: body.to_string(),
            hour,
            verdict,
            at: at.to_string(),
            kind,
        });
    }
    nudge();
}

/// Muted grey while a freshly selected location is fetching. Drops any balloon
/// that has not been shown yet, because it names the previous location: the
/// widget's planner is reset for the same reason (EXP17, EXP30).
pub fn set_loading(location: &str) {
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.color = theme::MUTED;
        state.tip = format!("AirQlality \u{00b7} {location} \u{00b7} fetching\u{2026}");
        state.pending = None;
    }
    nudge();
}

/// Muted grey + "refreshing..." while a manual refresh is in flight (EXP22).
///
/// Unlike [`set_loading`] this **keeps** any queued balloon: the refresh is
/// about the same place, and the planner's warning (EXP30) is still valid.
pub fn set_refreshing(location: &str) {
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.color = theme::MUTED;
        state.tip = format!("AirQlality \u{00b7} {location} \u{00b7} refreshing\u{2026}");
    }
    nudge();
}

/// Muted grey after a fetch round that left nothing to show: not "fetching"
/// (nothing is in flight) and not a verdict (there is none). Before EXP22 a cold
/// start that failed left the bare `AirQlality` tip.
pub fn set_no_data(location: &str) {
    {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.color = theme::MUTED;
        state.tip = format!("AirQlality \u{00b7} {location} \u{00b7} no data");
    }
    nudge();
}

/// Ask the tray thread to repaint the icon / flush a pending balloon. Safe to
/// call before [`start`]; the thread picks up the current state when it creates
/// the icon.
fn nudge() {
    let hwnd = TRAY_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut c_void)),
                WM_TRAY_REFRESH,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

/// What a window-close request should do (pure, so it is unit-testable).
#[derive(Debug, PartialEq, Eq)]
pub enum CloseAction {
    /// Let the window close: the app is really quitting.
    Quit,
    /// Swallow the close and hide to the tray.
    Hide,
}

/// Decide what to do with a close request. Quit wins when the user asked to quit
/// or when there is no tray icon to restore the window from.
pub fn close_action(quit_requested: bool, tray_ready: bool) -> CloseAction {
    if quit_requested || !tray_ready {
        CloseAction::Quit
    } else {
        CloseAction::Hide
    }
}

/// The toggle menu item's label for the current window visibility.
pub fn toggle_label(visible: bool) -> &'static str {
    if visible {
        "Hide"
    } else {
        "Show"
    }
}

/// The flags for one "Notify me on" check (EXP28): a native checkmark when the
/// verdict is selected, plain text when it is not.
pub fn notify_item_flags(checked: bool) -> MENU_ITEM_FLAGS {
    if checked {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    }
}

/// The three entries of the "Notify me on" submenu in menu order (EXP28):
/// command id, label, checked. The label is the user's wording, not the
/// verdict's uppercase label: the menu is a setting, not an alarm.
pub fn notify_menu_items(targets: &notify::Targets) -> [(u32, &'static str, bool); 3] {
    [
        (
            MENU_NOTIFY_CLOSED,
            "Closing the window",
            targets.closed(),
        ),
        (MENU_NOTIFY_OPEN, "Opening the window", targets.open()),
        (MENU_NOTIFY_VENT, "Venting the window", targets.vent()),
    ]
}

/// Build the real submenu from the stored targets, so the checks the user sees
/// and the planner reads come from one source (EXP28).
unsafe fn notify_submenu(targets: &notify::Targets) -> Option<HMENU> {
    let submenu = CreatePopupMenu().ok()?;
    for (id, label, checked) in notify_menu_items(targets) {
        let text = wide(label);
        let _ = AppendMenuW(
            submenu,
            notify_item_flags(checked),
            id as usize,
            PCWSTR(text.as_ptr()),
        );
    }
    Some(submenu)
}

/// Flip one notification choice from the menu and persist it (EXP28). Works
/// while the widget is hidden: the planner reads this same state.
fn toggle_notify(verdict: Verdict) {
    let targets = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let mut targets = state.targets();
        targets.toggle(verdict);
        state.targets = Some(targets);
        targets
    };
    persist::save_notify(&targets);
}

/// The flags for the "Run at startup" menu item: a native checkmark when the
/// current executable is registered to run at login (EXP12).
pub fn startup_menu_flags(registered: bool) -> MENU_ITEM_FLAGS {
    if registered {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING
    }
}

/// The flags for a menu item that reaches the widget (Refresh, EXP22; API
/// keys, EXP24): greyed when no widget is listening. An enabled item that does
/// nothing would be a lie.
pub fn menu_item_flags(available: bool) -> MENU_ITEM_FLAGS {
    if available {
        MF_STRING
    } else {
        MF_STRING | MF_GRAYED
    }
}

/// Called by gpui on `WM_CLOSE` (the titlebar X / Alt+F4). Returns `true` to let
/// the window close; `false` swallows the close and hides to the tray instead.
pub fn should_close() -> bool {
    match close_action(
        QUIT_REQUESTED.load(Ordering::SeqCst),
        TRAY_READY.load(Ordering::SeqCst),
    ) {
        CloseAction::Quit => true,
        CloseAction::Hide => {
            unsafe { hide_main() };
            false
        }
    }
}

/// Ask the app to quit for real: flag the close and replay it through gpui's
/// `on_window_should_close`, which then allows the window to close.
fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

/// Hide the widget; it stays alive and the tray icon keeps updating.
unsafe fn hide_main() {
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        let _ = ShowWindow(HWND(hwnd as *mut c_void), SW_HIDE);
    }
}

/// Show and focus the widget.
unsafe fn show_main() {
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    if hwnd == 0 {
        return;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    if IsWindow(Some(hwnd)).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Toggle the widget's visibility (the menu's Show/Hide item).
unsafe fn toggle_main() {
    let hwnd = MAIN_HWND.load(Ordering::SeqCst);
    if hwnd == 0 {
        return;
    }
    let hwnd = HWND(hwnd as *mut c_void);
    if IsWindowVisible(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_HIDE);
    } else {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Pop up the tray context menu (Show/Hide, Refresh, API keys, Run at startup,
/// Quit) at the cursor.
unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    let main = MAIN_HWND.load(Ordering::SeqCst);
    let visible = main != 0 && IsWindowVisible(HWND(main as *mut c_void)).as_bool();
    let toggle = wide(toggle_label(visible));
    let refresh_label = wide("Refresh");
    let keys_label = wide("API keys...");
    let notify_label = wide(NOTIFY_MENU);
    let startup_label = wide("Run at startup");
    let quit = wide("Quit");
    let _ = AppendMenuW(menu, MF_STRING, MENU_TOGGLE as usize, PCWSTR(toggle.as_ptr()));
    // Greyed unless the widget published its request channel (EXP22).
    let _ = AppendMenuW(
        menu,
        menu_item_flags(commands::available(Command::Refresh)),
        MENU_REFRESH as usize,
        PCWSTR(refresh_label.as_ptr()),
    );
    // Always available: unlike Refresh, the prompts have something to do in
    // every mode, including the mock seams (EXP24).
    let _ = AppendMenuW(
        menu,
        menu_item_flags(commands::available(Command::ApiKeys)),
        MENU_KEYS as usize,
        PCWSTR(keys_label.as_ptr()),
    );
    // Which verdicts may notify: a real submenu of native checks (EXP28). The
    // checks are read from the same state the planner uses, so they cannot lie.
    let targets = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.targets()
    };
    if let Some(submenu) = notify_submenu(&targets) {
        let _ = AppendMenuW(
            menu,
            MF_POPUP,
            submenu.0 as usize,
            PCWSTR(notify_label.as_ptr()),
        );
    }
    // A native checkmark shows whether we are registered for login startup.
    let _ = AppendMenuW(
        menu,
        startup_menu_flags(startup::is_registered()),
        MENU_STARTUP as usize,
        PCWSTR(startup_label.as_ptr()),
    );
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR(std::ptr::null()));
    let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT as usize, PCWSTR(quit.as_ptr()));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    // Required so the menu dismisses when the user clicks elsewhere.
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        pt.x,
        pt.y,
        None,
        hwnd,
        None,
    )
    .0;
    let _ = DestroyMenu(menu);
    let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));

    match cmd as u32 {
        MENU_TOGGLE => toggle_main(),
        MENU_REFRESH => {
            commands::request(Command::Refresh);
        }
        MENU_KEYS => {
            // The prompts live in the widget, so the menu item must make the
            // widget visible first, then ask it to open step 1 (EXP24).
            show_main();
            commands::request(Command::ApiKeys);
        }
        MENU_STARTUP => {
            startup::toggle();
        }
        MENU_NOTIFY_CLOSED => toggle_notify(Verdict::KeepClosed),
        MENU_NOTIFY_OPEN => toggle_notify(Verdict::Open),
        MENU_NOTIFY_VENT => toggle_notify(Verdict::Vent),
        MENU_QUIT => request_quit(),
        _ => {}
    }
}

/// Test seam: `AIRQLALITY_TRAY_QUIT_MS=<n>` triggers the same quit path the menu
/// uses after `n` ms, so the E2E can exercise a real quit without clicking.
fn maybe_spawn_quit_seam() {
    let Some(ms) = std::env::var("AIRQLALITY_TRAY_QUIT_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return;
    };
    let _ = thread::Builder::new()
        .name("airqlality-tray-quit-seam".into())
        .spawn(move || {
            thread::sleep(std::time::Duration::from_millis(ms));
            request_quit();
        });
}

/// Test seam: `AIRQLALITY_TRAY_REFRESH_MS=<n>` fires the same request the
/// "Refresh" item does after `n` ms, so the E2E can exercise the menu path
/// without a real right-click (EXP22).
fn maybe_spawn_refresh_seam() {
    let Some(ms) = std::env::var("AIRQLALITY_TRAY_REFRESH_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
    else {
        return;
    };
    let _ = thread::Builder::new()
        .name("airqlality-tray-refresh-seam".into())
        .spawn(move || {
            thread::sleep(std::time::Duration::from_millis(ms));
            commands::request(Command::Refresh);
        });
}

/// The tray thread: register a class, create the hidden window, add the icon and
/// pump messages until the process exits.
unsafe fn run(main_hwnd: isize) {
    MAIN_HWND.store(main_hwnd, Ordering::SeqCst);

    let hinstance = match GetModuleHandleW(None) {
        Ok(h) => HINSTANCE(h.0),
        Err(_) => HINSTANCE::default(),
    };
    let class = wide("AirQlalityTrayWindow");
    let title = wide("AirQlality");

    let wc = WNDCLASSW {
        hInstance: hinstance,
        lpszClassName: PCWSTR(class.as_ptr()),
        lpfnWndProc: Some(wndproc),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let hwnd = match CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        PCWSTR(class.as_ptr()),
        PCWSTR(title.as_ptr()),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        Some(hinstance),
        None,
    ) {
        Ok(hwnd) => hwnd,
        Err(_) => return,
    };
    TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

    add_icon(hwnd);

    let mut msg = MSG::default();
    loop {
        // 0 = WM_QUIT, -1 = error; both end the loop.
        let ret = GetMessageW(&mut msg, None, 0, 0).0;
        if ret == 0 || ret == -1 {
            break;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

/// `NIM_ADD` the icon with the current state.
unsafe fn add_icon(hwnd: HWND) {
    let (color, tip) = current();
    let icon = make_hicon(color);
    let mut nid = base_nid(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    nid.szTip = tip_array(&tip);
    let added = Shell_NotifyIconW(NIM_ADD, &nid).as_bool();
    if added {
        // Only now can a hidden window be restored from the tray.
        TRAY_READY.store(true, Ordering::SeqCst);
        maybe_spawn_quit_seam();
        maybe_spawn_refresh_seam();
        startup::maybe_toggle_seam();
    }
    // Version 4 makes LOWORD(lParam) the event, so clicks are decoded correctly.
    nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    let _ = Shell_NotifyIconW(NIM_SETVERSION, &nid);
    if added {
        // The first verdict can be evaluated before the icon exists; deliver its
        // warning now (EXP17).
        flush_pending(hwnd);
    }
    let _ = DestroyIcon(icon); // the shell keeps its own copy
    dump(color, &tip, added);
}

/// Rebuild the `HICON` from the current state and `NIM_MODIFY` it.
unsafe fn refresh_icon(hwnd: HWND) {
    let (color, tip) = current();
    let icon = make_hicon(color);
    let mut nid = base_nid(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
    nid.hIcon = icon;
    nid.szTip = tip_array(&tip);
    let ok = Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool();
    let _ = DestroyIcon(icon);
    dump(color, &tip, ok);
    // A verdict change while the widget is hidden still reaches the user.
    flush_pending(hwnd);
}

/// Send a queued balloon notification (`NIF_INFO`) and clear it, so it is never
/// shown twice (EXP17). A fresh closed run queues a new one.
unsafe fn flush_pending(hwnd: HWND) {
    let info = {
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        state.pending.take()
    };
    let Some(pending) = info else {
        return;
    };
    let mut nid = base_nid(hwnd);
    nid.uFlags = NIF_INFO;
    nid.szInfoTitle = title_array(&pending.title);
    nid.szInfo = info_array(&pending.body);
    // A closed window is a warning; the optional opening/venting notes are
    // information (EXP28). All honour Focus Assist / quiet time and keep the
    // default notification sound.
    nid.dwInfoFlags = if pending.verdict == Verdict::KeepClosed {
        NIIF_WARNING | NIIF_RESPECT_QUIET_TIME
    } else {
        NIIF_INFO | NIIF_RESPECT_QUIET_TIME
    };
    let ok = Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool();
    dump_notify(
        pending.verdict,
        &pending.title,
        &pending.body,
        pending.hour,
        &pending.at,
        pending.kind,
        ok,
    );
}

/// `NIM_DELETE` the icon (called from `WM_DESTROY`).
unsafe fn remove_icon(hwnd: HWND) {
    let nid = base_nid(hwnd);
    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
}

/// A `NOTIFYICONDATAW` with just the identity fields set.
fn base_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uCallbackMessage: WM_TRAY,
        ..Default::default()
    }
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            // Version 4: LOWORD(lParam) carries the mouse event.
            match (lparam.0 as u32) & 0xffff {
                WM_LBUTTONUP => show_main(),
                WM_RBUTTONUP => show_menu(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_TRAY_REFRESH => {
            refresh_icon(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            remove_icon(hwnd);
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Build a coloured-square `HICON` at the system small-icon size.
unsafe fn make_hicon(color: u32) -> HICON {
    let size = icon_size();
    // The DIB -> HICON plumbing is shared with the app icon (`icon.rs`).
    unsafe { crate::icon::hicon_from_bgra(&icon_bgra(color, size), size) }
}

/// System small-icon size (a crisp tray square on the primary monitor's DPI).
unsafe fn icon_size() -> u32 {
    let w = GetSystemMetrics(SM_CXSMICON);
    let h = GetSystemMetrics(SM_CYSMICON);
    w.max(h).clamp(16, 64) as u32
}

/// A copy of the desired state.
fn current() -> (u32, String) {
    let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let tip = if state.tip.is_empty() {
        "AirQlality".to_string()
    } else {
        state.tip.clone()
    };
    (state.color, tip)
}

/// A null-terminated UTF-16 buffer for the Win32 `...W` entry points.
fn wide(s: &str) -> Vec<u16> {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    buf.push(0);
    buf
}

/// A `NOTIFYICONDATAW::szTip` (128 UTF-16 code units, null-padded).
fn tip_array(s: &str) -> [u16; 128] {
    let mut buf = [0u16; 128];
    for (i, unit) in s.encode_utf16().take(127).enumerate() {
        buf[i] = unit;
    }
    buf
}

/// A `NOTIFYICONDATAW::szInfo` (256 UTF-16 code units, null-padded).
fn info_array(s: &str) -> [u16; 256] {
    let mut buf = [0u16; 256];
    for (i, unit) in s.encode_utf16().take(255).enumerate() {
        buf[i] = unit;
    }
    buf
}

/// A `NOTIFYICONDATAW::szInfoTitle` (64 UTF-16 code units, null-padded).
fn title_array(s: &str) -> [u16; 64] {
    let mut buf = [0u16; 64];
    for (i, unit) in s.encode_utf16().take(63).enumerate() {
        buf[i] = unit;
    }
    buf
}

/// Test seam: with `AIRQLALITY_TRAY_DUMP=<path>`, write the icon state after
/// every `NIM_ADD`/`NIM_MODIFY` so the E2E can assert the verdict->colour wiring.
fn dump(color: u32, tip: &str, ok: bool) {
    let Some(path) = std::env::var_os("AIRQLALITY_TRAY_DUMP") else {
        return;
    };
    let escaped = tip.replace('\\', "\\\\").replace('"', "\\\"");
    let json = format!("{{\"color\":\"#{color:06x}\",\"tip\":\"{escaped}\",\"ok\":{ok}}}");
    let _ = std::fs::write(path, json);
}

/// Test seam: with `AIRQLALITY_NOTIFY_DUMP=<path>`, append one JSON line per
/// balloon actually sent, so the E2E can assert the planner fired and the shell
/// accepted the call, at the Win32 boundary (EXP17, EXP27, EXP30). `verdict`
/// says *which* promise the balloon belongs to (EXP28), `hour` is the warned
/// run's first hour, `at` the evaluation instant and `kind` whether the warning
/// was ahead of, at, or after its run started.
fn dump_notify(
    verdict: Verdict,
    title: &str,
    body: &str,
    hour: i64,
    at: &str,
    kind: &str,
    ok: bool,
) {
    let Some(path) = std::env::var_os("AIRQLALITY_NOTIFY_DUMP") else {
        return;
    };
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let verdict = match verdict {
        Verdict::KeepClosed => "closed",
        Verdict::Vent => "vent",
        Verdict::Open => "open",
    };
    let line = format!(
        "{{\"verdict\":\"{verdict}\",\"hour\":{hour},\"at\":\"{}\",\"kind\":\"{kind}\",\"title\":\"{}\",\"body\":\"{}\",\"ok\":{ok}}}\n",
        esc(at),
        esc(title),
        esc(body)
    );
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{GetMenuState, MF_BYCOMMAND};

    #[test]
    fn color_matches_the_widget_palette() {
        assert_eq!(color_for(Verdict::Open), 0x3fb950);
        assert_eq!(color_for(Verdict::Vent), 0xd29922);
        assert_eq!(color_for(Verdict::KeepClosed), 0xf85149);
    }

    #[test]
    fn icon_has_a_white_border_and_a_coloured_centre() {
        let px = icon_bgra(0x3fb950, 4);
        assert_eq!(px.len(), 4 * 4 * 4);
        // Top-left corner is the white border (BGRA).
        assert_eq!(&px[0..4], &[0xff, 0xff, 0xff, 0xff]);
        // Centre (x = 2, y = 2) is the verdict colour 0x3fb950 -> B,G,R,A.
        let centre = (2 * 4 + 2) * 4;
        assert_eq!(&px[centre..centre + 4], &[0x50, 0xb9, 0x3f, 0xff]);
    }

    #[test]
    fn a_zero_size_still_yields_one_pixel() {
        assert_eq!(icon_bgra(0, 0).len(), 4);
    }

    #[test]
    fn close_hides_unless_quitting_or_no_tray() {
        assert_eq!(close_action(false, true), CloseAction::Hide);
        assert_eq!(close_action(true, true), CloseAction::Quit);
        assert_eq!(close_action(false, false), CloseAction::Quit);
        assert_eq!(close_action(true, false), CloseAction::Quit);
    }

    #[test]
    fn toggle_label_reflects_visibility() {
        assert_eq!(toggle_label(true), "Hide");
        assert_eq!(toggle_label(false), "Show");
    }

    #[test]
    fn startup_item_is_checked_only_when_registered() {
        use windows::Win32::UI::WindowsAndMessaging::{MF_CHECKED, MF_STRING};
        assert!(startup_menu_flags(true).contains(MF_CHECKED));
        assert!(startup_menu_flags(true).contains(MF_STRING));
        assert!(!startup_menu_flags(false).contains(MF_CHECKED));
        assert!(startup_menu_flags(false).contains(MF_STRING));
    }

    #[test]
    fn refresh_item_is_greyed_only_without_a_listener() {
        use windows::Win32::UI::WindowsAndMessaging::MF_STRING;
        assert!(menu_item_flags(true).contains(MF_STRING));
        assert!(!menu_item_flags(true).contains(MF_GRAYED));
        assert!(menu_item_flags(false).contains(MF_STRING));
        assert!(menu_item_flags(false).contains(MF_GRAYED));
    }

    #[test]
    fn notify_items_are_the_users_order_and_wording() {
        let targets = notify::Targets::parse("closed,vent");
        assert_eq!(
            notify_menu_items(&targets),
            [
                (MENU_NOTIFY_CLOSED, "Closing the window", true),
                (MENU_NOTIFY_OPEN, "Opening the window", false),
                (MENU_NOTIFY_VENT, "Venting the window", true),
            ]
        );
        assert_eq!(NOTIFY_MENU, "Notify me on");
        assert!(notify_item_flags(true).contains(MF_CHECKED));
        assert!(!notify_item_flags(false).contains(MF_CHECKED));
    }

    /// EXP28: the real submenu the tray appends, read back from Win32. Proves
    /// the checks match the stored choices without opening a window.
    #[test]
    fn the_notify_submenu_checks_the_saved_choices() {
        let targets = notify::Targets::parse("open,vent");
        unsafe {
            let submenu = notify_submenu(&targets).expect("create the notify submenu");
            for (id, label, checked) in notify_menu_items(&targets) {
                let state = GetMenuState(submenu, id, MF_BYCOMMAND);
                assert_ne!(state, u32::MAX, "menu item {id} ({label}) is missing");
                assert_eq!(
                    state & MF_CHECKED.0 != 0,
                    checked,
                    "checkmark of {label} disagrees with the saved choices"
                );
            }
            let _ = DestroyMenu(submenu);
        }
    }

    /// EXP30: the tray only transports. `set_verdict` recolours and never
    /// queues a balloon (the planner decides); a loading drops a queued balloon
    /// because it names the previous place, while a refresh keeps it.
    #[test]
    fn a_verdict_only_recolours_and_a_loading_drops_a_pending_balloon() {
        set_verdict(Verdict::KeepClosed, "Cairo, Egypt");
        {
            let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(state.color, theme::CLOSED);
            assert!(state.tip.contains("KEEP CLOSED"), "tip: {}", state.tip);
            assert!(
                state.pending.is_none(),
                "a plain verdict change must not warn; the planner does"
            );
        }

        warn(
            Verdict::KeepClosed,
            "KEEP THE WINDOW CLOSED",
            "Cairo, Egypt \u{b7} PM2.5 30 \u{b5}g/m\u{b3} \u{b7} from 4:00 PM (in 10 min)",
            100,
            "2026-01-01T15:50+02:00",
            "ahead",
        );
        {
            let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            let pending = state.pending.as_ref().expect("the balloon was queued");
            assert_eq!(pending.kind, "ahead");
            assert_eq!(pending.hour, 100);
        }

        set_refreshing("Cairo, Egypt");
        {
            let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(state.color, theme::MUTED);
            assert!(state.tip.contains("refreshing"));
            assert!(
                state.pending.is_some(),
                "a refresh is the same place: its warning survives"
            );
        }

        set_loading("Cairo, Egypt");
        {
            let state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(state.color, theme::MUTED);
            assert!(
                state.pending.is_none(),
                "a location switch must drop the previous place's balloon"
            );
        }
    }
}
