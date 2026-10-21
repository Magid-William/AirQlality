//! Open a URL in the user's default browser (EXP24).
//!
//! gpui has no "open link" API, and the free-key hints in the API-key prompt
//! have to be real links: nobody should retype an address they can click.
//! `ShellExecuteW` with the "open" verb is the documented way to hand a URL to
//! the shell, and it needs no window handle. Best effort: a failure (no
//! handler, blocked by policy) is ignored, like the other platform niceties.

/// Open `url` with the system's default handler.
#[cfg(windows)]
pub fn open(url: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = wide("open");
    let target = wide(url);
    // SAFETY: both strings are NUL-terminated and outlive the call, and a URL
    // needs no window handle or parameters - that is exactly what the shell
    // expects for "open in the default browser".
    unsafe {
        let _ = ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Non-Windows build: the widget is Windows-only, but the crate still has to
/// compile elsewhere for the pure tests.
#[cfg(not(windows))]
pub fn open(_url: &str) {}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
