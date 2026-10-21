//! Run-at-startup registration via the per-user `HKCU\...\Run` key (EXP12).
//!
//! The app registers itself (opt-in, from the tray menu) by writing one `REG_SZ`
//! value whose data is the quoted current executable plus [`STARTUP_ARG`], so a
//! login launch starts hidden in the tray. No admin rights are needed and no new
//! crate: this uses the already-present `windows` crate
//! (`Win32_System_Registry`).
//!
//! The interesting logic — what command we write and whether a stored value
//! counts as "registered" — lives in the pure [`command_for`] / [`is_enabled`],
//! so it is unit-testable without touching the registry. The value name defaults
//! to `AirQlality` and can be overridden with `AIRQLALITY_STARTUP_NAME` so tests
//! never touch the real entry.

#![cfg(windows)]

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
};

/// The per-user Run key; `HKCU` so it works without elevation.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// The command-line flag appended so a login launch boots hidden in the tray.
pub const STARTUP_ARG: &str = "--startup";
/// The Run value name, overridable for tests via `AIRQLALITY_STARTUP_NAME`.
const DEFAULT_NAME: &str = "AirQlality";

/// Whether a startup entry applies to the current process (checked in `main`).
pub fn launched_at_startup() -> bool {
    std::env::args().any(|a| a == STARTUP_ARG)
}

/// The exact command written to the Run key for `exe`: the path (always quoted),
/// a space, then [`STARTUP_ARG`].
pub fn command_for(exe: &Path) -> String {
    format!("\"{}\" {STARTUP_ARG}", normalized_path(exe).display())
}

/// Whether the stored Run value would launch exactly `exe` at login. Trimmed and
/// compared case-insensitively (Windows paths are case-insensitive). A missing,
/// stale or foreign value is not ours.
pub fn is_enabled(stored: Option<&str>, exe: &Path) -> bool {
    match stored {
        Some(s) => s.trim().to_ascii_lowercase() == command_for(exe).to_ascii_lowercase(),
        None => false,
    }
}

/// Strip a Windows verbatim prefix (`\\?\` / `\\?\UNC\`) so the path is valid
/// both for the Run key and for the loader that parses the command.
fn normalized_path(exe: &Path) -> PathBuf {
    let s = exe.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        PathBuf::from(s.into_owned())
    }
}

fn value_name() -> String {
    std::env::var("AIRQLALITY_STARTUP_NAME").unwrap_or_else(|_| DEFAULT_NAME.to_string())
}

/// Whether the current executable is currently registered to run at login.
pub fn is_registered() -> bool {
    match current_exe() {
        Some(exe) => is_enabled(read_value(&value_name()).as_deref(), &exe),
        None => false,
    }
}

/// Register (`true`) or unregister (`false`) the current executable. Failures are
/// swallowed: best-effort, like the location persistence.
pub fn set_registered(on: bool) {
    let Some(exe) = current_exe() else { return };
    if on {
        write_value(&value_name(), &command_for(&exe));
    } else {
        delete_value(&value_name());
    }
}

/// Flip the registration for the current executable and report the new state
/// (what the tray menu item does).
pub fn toggle() -> bool {
    let now = !is_registered();
    set_registered(now);
    now
}

/// Test seam: `AIRQLALITY_STARTUP_TOGGLE=1` replays the menu's toggle once at
/// start-up (so the E2E can exercise it without clicking), then writes
/// `AIRQLALITY_STARTUP_DUMP`.
pub fn maybe_toggle_seam() {
    if std::env::var("AIRQLALITY_STARTUP_TOGGLE").ok().as_deref() != Some("1") {
        return;
    }
    let registered = toggle();
    dump(registered);
}

fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().ok()
}

fn ok(err: WIN32_ERROR) -> bool {
    err == ERROR_SUCCESS
}

/// Open the per-user Run key with the given access; `None` if it cannot be opened.
fn open_run_key(access: REG_SAM_FLAGS) -> Option<HKEY> {
    let mut hkey = HKEY::default();
    let path = wide(RUN_KEY);
    let err = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            Some(0),
            access,
            &mut hkey,
        )
    };
    ok(err).then_some(hkey)
}

/// Read a `REG_SZ` value as a `String` (`None` if absent/unreadable).
fn read_value(name: &str) -> Option<String> {
    let hkey = open_run_key(KEY_QUERY_VALUE)?;
    let name = wide(name);
    let mut size: u32 = 0;
    let mut ty = REG_VALUE_TYPE::default();
    let err = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            None,
            Some(&mut size),
        )
    };
    if !ok(err) || size == 0 {
        unsafe {
            let _ = RegCloseKey(hkey);
        }
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let err = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut ty),
            Some(buf.as_mut_ptr()),
            Some(&mut size),
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if !ok(err) {
        return None;
    }
    // REG_SZ is UTF-16LE; drop the trailing NUL.
    let units: Vec<u16> = buf[..size as usize]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
}

/// Write a `REG_SZ` value (Creating/replacing it).
fn write_value(name: &str, value: &str) {
    let Some(hkey) = open_run_key(KEY_SET_VALUE) else {
        return;
    };
    let name = wide(name);
    let mut bytes: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.extend_from_slice(&[0, 0]); // NUL terminator
    unsafe {
        let _ = RegSetValueExW(hkey, PCWSTR(name.as_ptr()), Some(0), REG_SZ, Some(&bytes));
        let _ = RegCloseKey(hkey);
    }
}

/// Delete a value if present (missing is fine).
fn delete_value(name: &str) {
    let Some(hkey) = open_run_key(KEY_SET_VALUE) else {
        return;
    };
    let name = wide(name);
    unsafe {
        let _ = RegDeleteValueW(hkey, PCWSTR(name.as_ptr()));
        let _ = RegCloseKey(hkey);
    }
}

/// A null-terminated UTF-16 buffer for the Win32 `...W` entry points.
fn wide(s: &str) -> Vec<u16> {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    buf.push(0);
    buf
}

/// Test seam: `AIRQLALITY_STARTUP_DUMP=<path>` records the app's own view of the
/// registration, so the E2E can compare it with the registry.
fn dump(registered: bool) {
    let Some(path) = std::env::var_os("AIRQLALITY_STARTUP_DUMP") else {
        return;
    };
    let command = current_exe()
        .map(|p| command_for(&p))
        .unwrap_or_default()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let json = format!("{{\"registered\":{registered},\"command\":\"{command}\"}}");
    let _ = std::fs::write(path, json);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_the_path_and_adds_the_flag() {
        let exe = Path::new(r"C:\Program Files\AirQlality\airqlality.exe");
        assert_eq!(
            command_for(exe),
            r#""C:\Program Files\AirQlality\airqlality.exe" --startup"#
        );
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        assert_eq!(
            command_for(Path::new(r"\\?\C:\apps\airqlality.exe")),
            r#""C:\apps\airqlality.exe" --startup"#
        );
        assert_eq!(
            command_for(Path::new(r"\\?\UNC\server\share\airqlality.exe")),
            r#""\\server\share\airqlality.exe" --startup"#
        );
    }

    #[test]
    fn registered_only_when_the_value_matches_our_exe() {
        let exe = Path::new(r"C:\apps\airqlality.exe");
        let wanted = command_for(exe);
        assert!(is_enabled(Some(&wanted), exe));
        // Case-insensitive and whitespace-tolerant.
        assert!(is_enabled(Some(&format!("  {}  ", wanted.to_uppercase())), exe));
        // A different path, a missing flag or a missing value are not ours.
        assert!(!is_enabled(Some(r#""C:\other\airqlality.exe" --startup"#), exe));
        assert!(!is_enabled(Some(r#""C:\apps\airqlality.exe""#), exe));
        assert!(!is_enabled(None, exe));
    }

    #[test]
    fn startup_arg_is_detected_by_exact_match() {
        // Pure membership check mirrors `launched_at_startup` without `argv`.
        let args = ["airqlality.exe", "--startup"];
        assert!(args.iter().any(|a| a == &STARTUP_ARG));
        assert!(!["airqlality.exe", "--startup=1"].iter().any(|a| a == &STARTUP_ARG));
    }
}
