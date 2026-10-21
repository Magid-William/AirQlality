//! AirQlality — a Windows widget that says when it is safe to open the window.
//!
//! This file is only the app entry point; the real work lives in
//! `airqlality-core` (model, data source, rule) and `ui` (the gpui view).

// Link as a Windows GUI application (PE subsystem 2), not a console app (3).
// Without this, Windows allocates a console window for the process, which shows
// up as an empty terminal behind the widget (EXP15). The app prints nothing at
// runtime, so there is no console output to preserve.
#![windows_subsystem = "windows"]

mod browser;
mod commands;
mod keys;
mod notify;
mod persist;
mod schedule;
mod theme;
mod ui;
#[cfg(windows)]
mod icon;
mod icon_design;
#[cfg(windows)]
mod startup;
#[cfg(windows)]
mod tray;

use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;

use ui::Widget;

fn main() {
    // A login launch (`--startup`, written into the Run key) opens hidden so the
    // widget lives in the tray until the user asks for it (EXP12).
    #[cfg(windows)]
    let launched_at_startup = startup::launched_at_startup();
    #[cfg(not(windows))]
    let launched_at_startup = false;

    application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(560.0), px(340.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("AirQlality — open-window widget".into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // Start hidden (and unfocused) on a login launch.
                    show: !launched_at_startup,
                    focus: !launched_at_startup,
                    ..Default::default()
                },
                |_window, cx| cx.new(|cx| Widget::new(cx)),
            )
            .expect("failed to open window");

        // The tray lives on its own thread; it needs the window's HWND to bring
        // the widget forward, and to hide it when the user closes it (EXP11).
        #[cfg(windows)]
        {
            let hwnd = window.update(cx, |_, window, _| window_hwnd(window)).unwrap_or(0);
            tray::start(hwnd);
            // Title bar + taskbar button show the four-quadrant app icon; the
            // executable carries the same icon embedded for Explorer (EXP14).
            icon::set_window_icon(hwnd);
            // Closing the window hides to the tray; only a real quit closes it.
            window
                .update(cx, |_, window, cx| {
                    window.on_window_should_close(cx, |_window, _cx| tray::should_close());
                })
                .ok();
        }

        // Do not steal focus on a login launch.
        if !launched_at_startup {
            cx.activate(true);
        }
    });
}

/// Read the native window handle out of the gpui window (`HasWindowHandle`).
#[cfg(windows)]
fn window_hwnd(window: &gpui::Window) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    // `gpui::Window` has an inherent `window_handle()` returning its
    // `AnyWindowHandle`; call the `HasWindowHandle` trait method explicitly.
    match HasWindowHandle::window_handle(window) {
        Ok(handle) => match handle.as_raw() {
            RawWindowHandle::Win32(w) => w.hwnd.get() as isize,
            _ => 0,
        },
        Err(_) => 0,
    }
}
