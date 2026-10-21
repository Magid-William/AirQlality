//! End-to-end test: launch the **real** widget binary on the deterministic mock
//! source, screenshot its Windows window, and assert the rendered verdict by
//! exact pixel colour.
//!
//! This is deliberately black-box: it does not import `airqlality-core` or the
//! app's internals, so a regression in the rule, the wiring, or the drawing all
//! show up here as a colour mismatch. It complements (does not replace) the
//! `airqlality-core` unit tests, which pin the exact verdicts.
//!
//! Run with: `cargo test -p airqlality --test e2e_widget`
#![cfg(windows)]

use std::path::Path;
use std::process::Command;

// The colours under test, mirrored from `app/src/theme.rs`. Re-declared here on
// purpose: the test checks the *rendered contract*, not the source of truth.
const BG: [u8; 3] = [0x0d, 0x11, 0x17];
const CLOSED: [u8; 3] = [0xf8, 0x51, 0x49];
const VENT: [u8; 3] = [0xd2, 0x99, 0x22];
const OPEN: [u8; 3] = [0x3f, 0xb9, 0x50];
/// The dropdown panel surface (`theme::PANEL`), only present when it is open.
const PANEL: [u8; 3] = [0x16, 0x1b, 0x22];
/// The keyboard focus ring (`theme::FOCUS`). Same value as the chart's forecast
/// blue, which only appears far below the header band these tests sample.
const FOCUS: [u8; 3] = [0x58, 0xa6, 0xff];
/// The pointer-hover crosshair (`theme::HOVER`), only painted while hovering.
const HOVER: [u8; 3] = [0xc9, 0xd1, 0xd9];
/// The inferred-noise curve (`theme::NOISE`), only painted with the overlay on.
const NOISE: [u8; 3] = [0xbc, 0x8c, 0xff];
/// The min–max spread envelope behind the fused curve (`theme::BAND`), only
/// painted when the stack has more than one source (EXP19).
const BAND: [u8; 3] = [0x1f, 0x3a, 0x5f];
/// The "sources disagree" chip (`theme::DISAGREE`), only painted when the
/// stack's spread straddles a verdict threshold (EXP19).
const DISAGREE: [u8; 3] = [0xff, 0xa6, 0x57];
/// The shaded night region behind the timeline (`theme::NIGHT`), only painted
/// with the day/night wave on (EXP21).
const NIGHT: [u8; 3] = [0x05, 0x07, 0x0d];
/// The observed (past) part of the PM2.5 curve (`theme::PAST`). Only the data
/// view paints it - the "COULDN'T FETCH" screen cannot - so it is the EXP22
/// proof that a recovered frame is really showing data (EXP22).
const PAST: [u8; 3] = [0x2f, 0x81, 0xf7];

/// Both tests open a real top-level window at the same centred position and
/// screenshot it, so they must never run at the same time (serially via lock).
static WINDOW_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn window_guard() -> std::sync::MutexGuard<'static, ()> {
    WINDOW_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Parse the `RECT=(x,y,w,h)` line the capture script prints.
fn rect_wh(stdout: &str) -> Option<(i64, i64)> {
    let line = stdout.lines().find(|l| l.starts_with("RECT=("))?;
    let inside = line.trim_start_matches("RECT=(").trim_end_matches(')');
    let parts: Vec<i64> = inside
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [_, _, w, h] => Some((*w, *h)),
        _ => None,
    }
}

#[test]
fn widget_e2e_renders_expected_verdict() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        // Pin the deterministic mock source and freeze the ticker, so the post
        // stays at the pinned "now" (index 23) and this screenshot stays
        // byte-identical regardless of capture latency. Without this the default
        // is the real Open-Meteo source.
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    // Smoke: the app must keep running long enough to be captured, and the
    // screenshot must come from the app window (not the full-screen fallback).
    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");
    assert!(png.exists(), "screenshot file missing after capture:\n{log}");
    assert!(!stdout.contains("RECT_FALLBACK"), "captured the desktop, not the window:\n{log}");
    let (w, h) = rect_wh(&stdout).unwrap_or_else(|| panic!("could not parse RECT:\n{log}"));
    assert!(
        (300..=1400).contains(&w) && (200..=1200).contains(&h),
        "implausible window rect {w}x{h}:\n{log}"
    );

    // Analyse the pixels.
    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());
    let top_rows = (ih as f32 * 0.35) as u32;

    let (mut closed, mut vent, mut open, mut bg) = (0usize, 0usize, 0usize, 0usize);
    let (mut top_closed, mut top_open) = (0usize, 0usize);
    let mut panel = 0usize;
    let mut night = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            match [px[0], px[1], px[2]] {
                CLOSED => {
                    closed += 1;
                    if y < top_rows {
                        top_closed += 1;
                    }
                }
                VENT => vent += 1,
                OPEN => {
                    open += 1;
                    if y < top_rows {
                        top_open += 1;
                    }
                }
                BG => bg += 1,
                PANEL => panel += 1,
                NIGHT => night += 1,
                _ => {}
            }
        }
    }
    eprintln!(
        "e2e pixels ({iw}x{ih}): closed={closed} vent={vent} open={open} bg={bg} \
         top_closed={top_closed} top_open={top_open} panel={panel} night={night}"
    );

    // The headline banner must say KEEP CLOSED: mock "now" (index 23) is
    // 26 ug/m3, so red belongs in the top band and green must not appear there.
    assert!(
        top_closed > 0,
        "expected KEEP CLOSED red in the headline band (top {top_rows} of {ih} rows)"
    );
    assert_eq!(
        top_open, 0,
        "unexpected OPEN green in the headline band - verdict/wiring regressed"
    );

    // Banner + per-hour ribbon must actually paint all three verdict colours.
    assert!(closed > 300, "too few CLOSED pixels: {closed}");
    assert!(open > 200, "too few OPEN pixels: {open}");
    assert!(vent > 300, "too few VENT pixels: {vent}");
    assert!(bg > 50_000, "canvas background missing: {bg}");
    assert_eq!(
        panel, 0,
        "dropdown panel surface painted while the dropdown should be closed"
    );
    // The day/night wave is off by default under the mock seam (EXP21), so the
    // pre-EXP21 baseline is untouched: no night pixel may appear here.
    assert_eq!(
        night, 0,
        "night shading painted while the day/night wave is off (AIRQLALITY_NIGHT)"
    );

    let _ = std::fs::remove_file(&png);
}

/// Hovering the chart with the pointer (a real posted `WM_MOUSEMOVE`) must raise
/// the tooltip: the panel surface and the crosshair appear, where the no-hover
/// screenshot has neither. EXP13.
#[test]
fn widget_e2e_hover_shows_tooltip() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-hover-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        // Over the middle of the chart, as a fraction of the client area. The
        // mock + freeze keep the hovered hour deterministic.
        .arg("-MouseFracX")
        .arg("0.5")
        .arg("-MouseFracY")
        .arg("0.55")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");
    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");
    assert!(png.exists(), "screenshot file missing after capture:\n{log}");

    let img = image::open(&png).expect("decode screenshot PNG").to_rgba8();
    let (iw, ih) = (img.width(), img.height());
    let (mut panel, mut hover) = (0usize, 0usize);
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            match [px[0], px[1], px[2]] {
                PANEL => panel += 1,
                HOVER => hover += 1,
                _ => {}
            }
        }
    }
    eprintln!("e2e hover ({iw}x{ih}): panel={panel} hover={hover}");

    // The tooltip panel is ~150x60 px; text covers some, so >500 is a low bar.
    assert!(
        panel > 500,
        "hover tooltip panel was not painted (panel={panel}) - hit-test or render regressed:\n{log}"
    );
    // The crosshair is a 1px line down the full plot height.
    assert!(
        hover > 20,
        "hover crosshair was not painted (hover={hover}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
}

/// The failure path: default (real) source pointed at a closed port must render
/// the red "COULDN'T FETCH" screen, not a blank window or stale success.
#[test]
fn widget_e2e_shows_fetch_error() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-err-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        // Real mode (the default), but an unreachable endpoint fails fast.
        .env("AIRQLALITY_OPENMETEO_URL", "http://127.0.0.1:1")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");
    assert!(png.exists(), "screenshot missing after capture:\n{log}");

    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());

    let (mut closed, mut top_open) = (0usize, 0usize);
    let top_rows = (ih as f32 * 0.35) as u32;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            match [px[0], px[1], px[2]] {
                CLOSED => closed += 1,
                OPEN if y < top_rows => top_open += 1,
                _ => {}
            }
        }
    }
    eprintln!("e2e error pixels ({iw}x{ih}): closed={closed} top_open={top_open}");

    assert!(closed > 0, "expected the red COULDN'T FETCH headline:\n{log}");
    assert_eq!(
        top_open, 0,
        "unexpected OPEN green in the headline band on the error screen:\n{log}"
    );

    let _ = std::fs::remove_file(&png);
}

/// The dropdown path: forced open via `AIRQLALITY_OPEN_DROPDOWN`, the panel
/// surface must actually paint. This bite-proofs a `uniform_list` rendered
/// inside the deferred overlay (the risky part of EXP07).
#[test]
fn widget_e2e_opens_location_dropdown() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png =
        std::env::temp_dir().join(format!("airqlality-e2e-open-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        // Deterministic mock, frozen "now", the panel forced open, and a query
        // that returns results so the list (not the hint) is exercised.
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_OPEN_DROPDOWN", "1")
        .env("AIRQLALITY_SEARCH", "cairo")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");
    assert!(png.exists(), "screenshot file missing after capture:\n{log}");

    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());

    let mut panel = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            if [px[0], px[1], px[2]] == PANEL {
                panel += 1;
            }
        }
    }
    eprintln!("e2e open pixels ({iw}x{ih}): panel={panel}");

    // The panel (search field + results list + attribution) must paint.
    assert!(
        panel > 20_000,
        "dropdown panel did not paint when open (panel={panel}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
}

/// Real keyboard input: post "tokyo" to the window and assert the results list
/// renders (a hint-only panel is ~29k px; a filled list is ~90k). This covers
/// the hand-rolled search field's key path end to end.
#[test]
fn widget_e2e_types_a_city_search() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png =
        std::env::temp_dir().join(format!("airqlality-e2e-typed-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("8")
        .arg("-Type")
        .arg("tokyo")
        // Panel open, empty query, so the *only* source of results is the typing.
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_OPEN_DROPDOWN", "1")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());

    let mut panel = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            if [px[0], px[1], px[2]] == PANEL {
                panel += 1;
            }
        }
    }
    eprintln!("e2e typed pixels ({iw}x{ih}): panel={panel}");

    // Typed input shrank the panel to a hint (~29k); a real result list is
    // ~90k. This only passes if the keystrokes actually reached the field.
    assert!(
        panel > 50_000,
        "typing did not produce a results list (panel={panel}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
}

/// A query with no hits must render the "No matching city" state (still a
/// painted panel), not an empty window or a panic.
#[test]
fn widget_e2e_shows_no_city_match() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png =
        std::env::temp_dir().join(format!("airqlality-e2e-nomatch-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_OPEN_DROPDOWN", "1")
        .env("AIRQLALITY_SEARCH", "zzzzzzzz")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());

    let mut panel = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            if [px[0], px[1], px[2]] == PANEL {
                panel += 1;
            }
        }
    }
    eprintln!("e2e no-match pixels ({iw}x{ih}): panel={panel}");

    // Field + "No matching city" hint + attribution.
    assert!(
        panel > 5_000,
        "no-match panel did not paint (panel={panel}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
}

/// Keyboard focus: posting a single Tab moves focus to the location trigger and
/// paints the focus ring. The closed-state header band has no `FOCUS` pixels
/// (the only blue in the widget is the chart, well below the band), so a hit
/// here can only be the ring. EXP10.
#[test]
fn widget_e2e_tab_shows_focus_ring() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-focus-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-Press")
        .arg("tab")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());

    // Sample only the header band (the focus ring lives at the top; the chart's
    // forecast blue starts much lower).
    let band = (ih as f32 * 0.32) as u32;
    let mut focus = 0usize;
    for y in 0..band {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] < 200 {
                continue;
            }
            if [px[0], px[1], px[2]] == FOCUS {
                focus += 1;
            }
        }
    }
    eprintln!("e2e focus pixels ({iw}x{ih}): focus_in_band={focus} (band={band})");

    assert!(
        focus > 0,
        "Tab did not paint a focus ring on the trigger (focus_in_band={focus}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
}

/// Arrow-key navigation + Enter selects the highlighted city and persists it.
/// The results for "san" are population-ranked, so the third hit is Sanaa
/// (GeoNames id 71137); two Downs must select it, not the first hit.
#[test]
fn widget_e2e_arrow_enter_selects_active_city() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-sel-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);
    // Isolate persistence from the developer's real %APPDATA%.
    let appdata = std::env::temp_dir().join(format!("airqlality-appdata-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-Press")
        .arg("down down enter")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_OPEN_DROPDOWN", "1")
        .env("AIRQLALITY_SEARCH", "san")
        .env("APPDATA", &appdata)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let saved = std::fs::read_to_string(appdata.join("AirQlality").join("location.txt"))
        .expect("location was not persisted");
    eprintln!("e2e selected location id: {}", saved.trim());
    assert_eq!(
        saved.trim(),
        "71137",
        "arrow keys + Enter did not select the 3rd result (Sanaa, 71137):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_dir_all(&appdata);
}

/// Screen-reader access: with accessibility activated (WM_GETOBJECT, what a
/// screen reader posts), the AccessKit tree names the location control, gives
/// the field its value and the list its rows, and reports both tab stops.
#[test]
fn widget_e2e_accessibility_tree() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-a11y-{}.png", std::process::id()));
    let json = std::env::temp_dir().join(format!("airqlality-e2e-a11y-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&json);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-A11y")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_OPEN_DROPDOWN", "1")
        .env("AIRQLALITY_SEARCH", "san")
        .env("AIRQLALITY_A11Y", &json)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");

    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let doc = std::fs::read_to_string(&json)
        .unwrap_or_else(|e| panic!("a11y tree not dumped ({e}):\n{log}"));

    for needle in [
        "\"role\": \"Button\"",
        "\"label\": \"Change location\"",
        "\"keyboard_shortcut\": \"Ctrl+K\"",
        "\"role\": \"TextInput\"",
        "\"label\": \"Search a city\"",
        "\"value\": \"san\"",
        "\"role\": \"ListBox\"",
        "\"role\": \"ListItem\"",
        "\"role\": \"Label\"",
        "\"label\": \"KEEP CLOSED\"",
        "\"role\": \"Image\"",
        // Trigger, API keys, search field, best-window clock (EXP25).
        "\"tab_stop_count\": 4",
        // The banner's time is a real control, not decoration (EXP25).
        "\"label\": \"Best window 2:00 AM to 6:00 AM, 5 hours. Switch to the 24-hour clock.\"",
    ] {
        assert!(
            doc.contains(needle),
            "a11y tree missing {needle}:\n{doc}"
        );
    }

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&json);
}

/// Tray icon (EXP11): with the deterministic mock frozen at "now" (index 23,
/// 26 ug/m3), the notification-area icon must be added (`ok`) in KEEP CLOSED
/// red. The app writes its icon state to `AIRQLALITY_TRAY_DUMP` on every
/// `Shell_NotifyIcon` add/modify, so this is black-box at the Win32 boundary.
#[test]
fn widget_e2e_tray_icon_is_closed_red() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-tray-{}.png", std::process::id()));
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-tray-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_TRAY_DUMP", &dump)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("tray dump not written ({e}):\n{log}"));
    eprintln!("e2e tray (closed): {state}");
    assert!(
        state.contains("\"color\":\"#f85149\""),
        "tray icon is not KEEP CLOSED red:\n{state}"
    );
    assert!(state.contains("KEEP CLOSED"), "tray tooltip lost the verdict:\n{state}");
    assert!(
        state.contains("\"ok\":true"),
        "Shell_NotifyIcon did not accept the icon:\n{state}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
}

/// Closing the window must hide it to the tray, not quit: the process stays
/// alive, the window is no longer visible, and the tray icon is still there.
/// EXP11.
#[test]
fn widget_e2e_close_hides_to_tray() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("tray.ps1");
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-hide-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&dump);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Mode")
        .arg("hide")
        .arg("-Dump")
        .arg(&dump)
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .output()
        .expect("failed to spawn tray.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("--- tray.ps1 (hide) ---\n{stdout}");
    assert!(
        stdout.contains("ALIVE=1 VISIBLE=0 DUMP=1"),
        "closing the window did not hide it while keeping the app + tray icon:\n{stdout}"
    );

    let _ = std::fs::remove_file(&dump);
}

/// The tray's Quit path must actually terminate the app (exit 0), not leave it
/// hidden and running. The quit seam replays the same `request_quit()` the menu
/// calls, through gpui's `on_window_should_close`. EXP11.
#[test]
fn widget_e2e_tray_quit_exits() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("tray.ps1");

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Mode")
        .arg("quit")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_TRAY_QUIT_MS", "2500")
        .output()
        .expect("failed to spawn tray.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    eprintln!("--- tray.ps1 (quit) ---\n{stdout}");
    assert!(
        stdout.contains("EXITED code=0"),
        "the tray Quit path did not exit the app cleanly:\n{stdout}"
    );
}

/// The colour actually follows the data: pin the mock to an Open hour (index
/// 36, inside the 35..=39 clean window) and the same icon must be green.
#[test]
fn widget_e2e_tray_icon_is_open_green() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-trayo-{}.png", std::process::id()));
    let dump =
        std::env::temp_dir().join(format!("airqlality-e2e-trayo-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_MOCK_NOW", "36")
        .env("AIRQLALITY_TRAY_DUMP", &dump)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("tray dump not written ({e}):\n{log}"));
    eprintln!("e2e tray (open): {state}");
    assert!(
        state.contains("\"color\":\"#3fb950\""),
        "tray icon did not turn OPEN green:\n{state}"
    );
    assert!(state.contains("OPEN"), "tray tooltip lost the verdict:\n{state}");
    assert!(
        state.contains("\"ok\":true"),
        "Shell_NotifyIcon did not accept the icon:\n{state}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
}

/// Path to the run-at-startup E2E script.
fn startup_script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("startup.ps1")
}

/// Run `startup.ps1` in `mode`, returning its stdout (the script isolates the
/// registry value under a test name and cleans up).
fn run_startup_script(mode: &str, dump: Option<&Path>) -> String {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(startup_script())
        .arg("-Exe")
        .arg(env!("CARGO_BIN_EXE_airqlality"))
        .arg("-Mode")
        .arg(mode);
    if let Some(dump) = dump {
        cmd.arg("-Dump").arg(dump);
    }
    let output = cmd.output().expect("failed to spawn startup.ps1");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("PROCESS_EXITED") && !stdout.contains("NO_WINDOW"),
        "startup.ps1 ({mode}) failed:\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    stdout
}

/// EXP12: toggling "Run at startup" writes an HKCU Run value whose command
/// launches the current exe with `--startup`, and the app's own view agrees.
#[test]
fn widget_e2e_startup_registers() {
    let _guard = window_guard();
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-startup-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&dump);

    let stdout = run_startup_script("on", Some(&dump));
    eprintln!("--- startup.ps1 (on) ---\n{stdout}");
    assert!(
        stdout.contains("REGISTERED=1"),
        "toggle-on did not register the app in HKCU Run:\n{stdout}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("startup dump not written ({e}):\n{stdout}"));
    assert!(
        state.contains("\"registered\":true") && state.contains("--startup"),
        "the app's own startup view is wrong:\n{state}"
    );

    let _ = std::fs::remove_file(&dump);
}

/// EXP12: a registered app toggles back off, leaving no Run value behind.
#[test]
fn widget_e2e_startup_unregisters() {
    let _guard = window_guard();
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-startup-off-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&dump);

    let stdout = run_startup_script("off", Some(&dump));
    eprintln!("--- startup.ps1 (off) ---\n{stdout}");
    assert!(
        stdout.contains("REGISTERED=0 VALUE=-"),
        "toggle-off did not remove the HKCU Run value:\n{stdout}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("startup dump not written ({e}):\n{stdout}"));
    assert!(
        state.contains("\"registered\":false"),
        "the app's own startup view is still registered:\n{state}"
    );

    let _ = std::fs::remove_file(&dump);
}

/// EXP12: a login launch (`--startup`) opens the window hidden so the widget
/// lives in the tray, but the process and its window exist to be restored.
#[test]
fn widget_e2e_startup_launch_is_hidden() {
    let _guard = window_guard();
    let stdout = run_startup_script("hidden", None);
    eprintln!("--- startup.ps1 (hidden) ---\n{stdout}");
    assert!(
        stdout.contains("ALIVE=1 VISIBLE=0"),
        "a --startup launch did not start hidden with a live window:\n{stdout}"
    );
}

/// EXP14: the executable carries exactly the four-quadrant icon resource, with
/// the theme's traffic-light colours in the expected quadrants. Black-box: it
/// reads the icon straight out of the built `.exe`, not the app's code.
#[test]
fn widget_e2e_exe_has_the_four_quadrant_icon() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("icon.ps1");

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .output()
        .expect("failed to spawn icon.ps1");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- icon.ps1 stdout ---\n{stdout}");

    assert!(
        stdout.contains("ICON_COUNT=1"),
        "the exe should embed exactly one icon:\n{log}"
    );
    // Traffic-light order: red TL, yellow TR, green BL, grey BR (theme values).
    for (quad, rgb) in [
        ("TL", "#F85149"),
        ("TR", "#D29922"),
        ("BL", "#3FB950"),
        ("BR", "#8B949E"),
    ] {
        let needle = format!("QUAD_{quad}={rgb}");
        assert!(stdout.contains(&needle), "icon missing {needle}:\n{log}");
    }
}

/// Read the PE `Subsystem` field out of the built executable: `2` = Windows GUI,
/// `3` = Windows console. Walks the DOS header (`e_lfanew` at 0x3C), the `PE\0\0`
/// signature (4 bytes) and the 20-byte COFF header to the optional header, where
/// `Subsystem` is a `u16` at offset 68 (same offset for PE32 and PE32+).
fn pe_subsystem(bytes: &[u8]) -> u16 {
    let u16_at = |o: usize| u16::from_le_bytes([bytes[o], bytes[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    assert_eq!(&bytes[0..2], b"MZ", "not a PE file: missing MZ");
    let pe = u32_at(0x3C) as usize;
    assert_eq!(&bytes[pe..pe + 4], b"PE\0\0", "not a PE file: missing signature");
    // optional header starts after the 4-byte signature + 20-byte COFF header.
    u16_at(pe + 24 + 68)
}

/// EXP15: the app is a GUI tool, so the executable must use the Windows GUI
/// subsystem. On the console subsystem (`3`) Windows allocates a console window,
/// which appears as the stray empty terminal behind the widget. Black-box: it
/// reads the PE header of the built `.exe`, not the app's source.
#[test]
fn widget_e2e_exe_is_gui_subsystem() {
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let bytes = std::fs::read(exe).expect("read built exe");
    let subsystem = pe_subsystem(&bytes);
    eprintln!("e2e pe subsystem: {subsystem} (exe={exe})");
    assert_eq!(
        subsystem, 2,
        "exe PE subsystem is {subsystem}, expected 2 (Windows GUI): a console \
         window will open behind the widget"
    );
}

/// EXP14: the running window installs the big and small icons via `WM_SETICON`
/// (read back with `WM_GETICON`), so the title bar and taskbar show them.
#[test]
fn widget_e2e_window_icon_is_set() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-icon-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .output()
        .expect("failed to spawn capture.ps1");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let handle = |key: &str| {
        stdout
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .map(|v| v.trim().to_string())
    };
    let big = handle("ICON_BIG=").unwrap_or_else(|| panic!("no ICON_BIG line:\n{log}"));
    let small = handle("ICON_SMALL=").unwrap_or_else(|| panic!("no ICON_SMALL line:\n{log}"));
    eprintln!("e2e window icon: big={big} small={small}");
    assert_ne!(big, "0", "WM_SETICON(ICON_BIG) did not stick:\n{log}");
    assert_ne!(small, "0", "WM_SETICON(ICON_SMALL) did not stick:\n{log}");

    let _ = std::fs::remove_file(&png);
}

/// EXP17: when the verdict is `KEEP CLOSED`, the app must raise exactly one
/// system notification. The app appends one JSON line per balloon actually sent
/// to `AIRQLALITY_NOTIFY_DUMP`, so this is black-box at the Win32 boundary:
/// it proves the gate fired and `Shell_NotifyIconW` accepted the call.
#[test]
fn widget_e2e_notifies_keep_closed() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-notify-{}.png", std::process::id()));
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-notify-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        // Frozen mock "now" (index 23, 26 ug/m3) -> KEEP CLOSED.
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_NOTIFY_DUMP", &dump)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("notify dump not written ({e}):\n{log}"));
    eprintln!("e2e notify (closed): {state}");
    let lines: Vec<&str> = state.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "expected exactly one notification for one closed reading:\n{state}"
    );
    assert!(
        lines[0].contains("\"ok\":true"),
        "Shell_NotifyIcon(NIF_INFO) was rejected:\n{state}"
    );
    assert!(
        lines[0].contains("\"title\":\"KEEP THE WINDOW CLOSED\"")
            && lines[0].contains("\"body\":\"Cairo, Egypt"),
        "notification must lead with the uppercase imperative and list the data after:\n{state}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
}

/// EXP17: the other half of the contract - an `OPEN` verdict must raise **no**
/// notification at all. Mock index 36 (14.5 ug/m3) is inside the clean window.
#[test]
fn widget_e2e_notify_is_silent_when_open() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png =
        std::env::temp_dir().join(format!("airqlality-e2e-notifyo-{}.png", std::process::id()));
    let dump =
        std::env::temp_dir().join(format!("airqlality-e2e-notifyo-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_MOCK_NOW", "36")
        .env("AIRQLALITY_NOTIFY_DUMP", &dump)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let state = std::fs::read_to_string(&dump).unwrap_or_default();
    assert!(
        state.trim().is_empty(),
        "an OPEN verdict must not notify, but the dump has:\n{state}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
}

/// EXP27: one balloon per **red segment**, not per evaluation. The synthetic
/// series walks `[open][vent][closed][closed][vent][closed][vent]` through the
/// real ticker: the two closed hours are one segment, the later closed hour is a
/// second, so exactly two notifications - and the two-hour run must not add a
/// third. `AIRQLALITY_MOCK_PM25` + `AIRQLALITY_TICK_MS` make the pattern play in
/// seconds; the clean tail keeps the ticker from wrapping back into it.
#[test]
fn widget_e2e_notify_once_per_red_segment() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!(
        "airqlality-e2e-notifyseg-{}.png",
        std::process::id()
    ));
    let dump = std::env::temp_dir().join(format!(
        "airqlality-e2e-notifyseg-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    // 12 open, 20 vent, 30 closed, 30 closed, 20 vent, 35 closed, 20 vent - then
    // a clean tail the harness cannot reach before it closes the app.
    let mut pattern = String::from("12,20,30,30,20,35,20");
    for _ in 0..40 {
        pattern.push_str(",12");
    }

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("3")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_MOCK_PM25", &pattern)
        .env("AIRQLALITY_TICK_MS", "200")
        .env("AIRQLALITY_NOTIFY_DUMP", &dump)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("notify dump not written ({e}):\n{log}"));
    eprintln!("e2e notify (segments): {state}");
    let lines: Vec<&str> = state.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        2,
        "two red segments must warn exactly twice, however long each runs:\n{state}"
    );
    for line in &lines {
        assert!(
            line.contains("\"ok\":true"),
            "Shell_NotifyIcon(NIF_INFO) was rejected:\n{state}"
        );
    }
    assert!(
        lines[0].contains("\"body\":\"Cairo, Egypt \u{b7} PM2.5 30 ")
            && lines[1].contains("\"body\":\"Cairo, Egypt \u{b7} PM2.5 35 "),
        "each warning must describe the reading that started its segment:\n{state}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
}

/// EXP27: the other half - a long closed run is still **one** segment. Six
/// closed hours in a row produce exactly one notification, never one per refresh
/// or per hour.
#[test]
fn widget_e2e_notify_long_red_run_is_one_warning() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!(
        "airqlality-e2e-notifylong-{}.png",
        std::process::id()
    ));
    let dump = std::env::temp_dir().join(format!(
        "airqlality-e2e-notifylong-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    let mut pattern = String::from("12,20,30,30,30,30,30,30");
    for _ in 0..40 {
        pattern.push_str(",12");
    }

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("3")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_MOCK_PM25", &pattern)
        .env("AIRQLALITY_TICK_MS", "200")
        .env("AIRQLALITY_NOTIFY_DUMP", &dump)
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let log = format!("--- capture stdout ---\n{stdout}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let state = std::fs::read_to_string(&dump)
        .unwrap_or_else(|e| panic!("notify dump not written ({e}):\n{log}"));
    eprintln!("e2e notify (long run): {state}");
    let lines: Vec<&str> = state.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        1,
        "a long closed run is one segment, so one warning:\n{state}"
    );
    assert!(
        lines[0].contains("\"ok\":true"),
        "Shell_NotifyIcon(NIF_INFO) was rejected:\n{state}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
}

/// The inferred noise overlay (EXP16). With `AIRQLALITY_NOISE=on` a second,
/// violet curve is painted on the timeline; the default (off under the mock
/// seam) paints none, so the pre-EXP16 screenshots stay byte-identical. Runs
/// with an isolated `%APPDATA%` so the location is the default Cairo, which has
/// baked road density.
#[test]
fn widget_e2e_shows_noise_overlay() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");

    let appdata = std::env::temp_dir().join(format!(
        "airqlality-noise-appdata-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");

    let run = |tag: &str, noise: &str| -> (usize, String) {
        let png = std::env::temp_dir().join(format!(
            "airqlality-e2e-noise-{tag}-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&png);
        let output = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg("-Exe")
            .arg(exe)
            .arg("-Out")
            .arg(&png)
            .env("AIRQLALITY_SOURCE", "mock")
            .env("AIRQLALITY_FREEZE_NOW", "1")
            .env("AIRQLALITY_NOISE", noise)
            .env("APPDATA", &appdata)
            .output()
            .expect("failed to spawn capture.ps1");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let log = format!("--- capture[{tag}] stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
        assert!(!stdout.contains("PROCESS_EXITED"), "app exited:\n{log}");
        assert!(stdout.contains("SAVED") && png.exists(), "no screenshot:\n{log}");

        let img = image::open(&png).expect("decode screenshot PNG").to_rgba8();
        let (iw, ih) = (img.width(), img.height());
        let mut count = 0usize;
        for y in 0..ih {
            for x in 0..iw {
                let px = img.get_pixel(x, y).0;
                if px[3] >= 200 && [px[0], px[1], px[2]] == NOISE {
                    count += 1;
                }
            }
        }
        let _ = std::fs::remove_file(&png);
        (count, log)
    };

    let (on, log_on) = run("on", "on");
    let (off, log_off) = run("off", "off");
    eprintln!("e2e noise ({exe}): on={on} off={off}");

    // A dashed 1.5px curve across ~700 px is well over 30 pixels.
    assert!(
        on > 30,
        "noise curve was not painted with AIRQLALITY_NOISE=on (noise={on}):\n{log_on}"
    );
    assert_eq!(
        off, 0,
        "noise curve painted while the overlay is off:\n{log_off}"
    );

    let _ = std::fs::remove_dir_all(&appdata);
}

/// The day/night wave (EXP21): with `AIRQLALITY_NIGHT=on` the hours between
/// 18:00 and 06:00 are shaded behind the curve, and with it off the same frame
/// paints not a single night pixel - which is what keeps every pre-EXP21
/// screenshot byte-identical. Runs with an isolated `%APPDATA%`.
#[test]
fn widget_e2e_shows_day_night_wave() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");

    let appdata = std::env::temp_dir().join(format!(
        "airqlality-night-appdata-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");

    let run = |tag: &str, night: &str| -> (usize, usize, usize, String) {
        let png = std::env::temp_dir().join(format!(
            "airqlality-e2e-night-{tag}-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&png);
        let output = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg("-Exe")
            .arg(exe)
            .arg("-Out")
            .arg(&png)
            .env("AIRQLALITY_SOURCE", "mock")
            .env("AIRQLALITY_FREEZE_NOW", "1")
            .env("AIRQLALITY_NIGHT", night)
            .env("APPDATA", &appdata)
            .output()
            .expect("failed to spawn capture.ps1");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let log = format!("--- capture[{tag}] stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
        assert!(!stdout.contains("PROCESS_EXITED"), "app exited:\n{log}");
        assert!(stdout.contains("SAVED") && png.exists(), "no screenshot:\n{log}");

        let img = image::open(&png).expect("decode screenshot PNG").to_rgba8();
        let (iw, ih) = (img.width(), img.height());
        let (mut night_px, mut closed, mut open) = (0usize, 0usize, 0usize);
        for y in 0..ih {
            for x in 0..iw {
                let px = img.get_pixel(x, y).0;
                if px[3] < 200 {
                    continue;
                }
                match [px[0], px[1], px[2]] {
                    NIGHT => night_px += 1,
                    CLOSED => closed += 1,
                    OPEN => open += 1,
                    _ => {}
                }
            }
        }
        let _ = std::fs::remove_file(&png);
        (night_px, closed, open, log)
    };

    let (on, on_closed, on_open, log_on) = run("on", "on");
    let (off, _, _, log_off) = run("off", "off");
    eprintln!("e2e night wave ({exe}): on={on} off={off} (closed={on_closed} open={on_open})");

    // Two night humps across ~645 device pixels, up to 45 % of the plot height.
    assert!(
        on > 5_000,
        "night region was not painted with AIRQLALITY_NIGHT=on (night={on}):\n{log_on}"
    );
    assert_eq!(
        off, 0,
        "night region painted while the wave is off:\n{log_off}"
    );
    // The wave is paint only: shading must not swallow the verdict colours.
    assert!(
        on_closed > 300 && on_open > 200,
        "the wave hid the verdict colours (closed={on_closed} open={on_open}):\n{log_on}"
    );

    let _ = std::fs::remove_dir_all(&appdata);
}

/// The stack (EXP19): with `AIRQLALITY_SOURCES=mock3` the widget fuses three
/// deterministic sources, paints the min–max spread envelope, and flags a
/// spread that straddles a verdict threshold. A single-source run must paint
/// neither, so the pre-EXP19 screenshots stay byte-identical. Runs with an
/// isolated `%APPDATA%` so no source keys are picked up.
#[test]
fn widget_e2e_stacked_sources_show_disagreement() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");

    let appdata = std::env::temp_dir().join(format!(
        "airqlality-stack-appdata-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");

    // Returns (band pixels, disagree pixels, capture log).
    let run = |tag: &str, sources: Option<&str>| -> ((usize, usize), String) {
        let png = std::env::temp_dir().join(format!(
            "airqlality-e2e-stack-{tag}-{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&png);
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg("-Exe")
            .arg(exe)
            .arg("-Out")
            .arg(&png)
            .env("AIRQLALITY_SOURCE", "mock")
            .env("AIRQLALITY_FREEZE_NOW", "1")
            .env("APPDATA", &appdata);
        if let Some(sources) = sources {
            cmd.env("AIRQLALITY_SOURCES", sources);
        }
        let output = cmd.output().expect("failed to spawn capture.ps1");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let log = format!("--- capture[{tag}] stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
        assert!(!stdout.contains("PROCESS_EXITED"), "app exited:\n{log}");
        assert!(stdout.contains("SAVED") && png.exists(), "no screenshot:\n{log}");

        let img = image::open(&png).expect("decode screenshot PNG").to_rgba8();
        let (iw, ih) = (img.width(), img.height());
        let (mut band, mut disagree) = (0usize, 0usize);
        for y in 0..ih {
            for x in 0..iw {
                let px = img.get_pixel(x, y).0;
                if px[3] < 200 {
                    continue;
                }
                match [px[0], px[1], px[2]] {
                    BAND => band += 1,
                    DISAGREE => disagree += 1,
                    _ => {}
                }
            }
        }
        let _ = std::fs::remove_file(&png);
        ((band, disagree), log)
    };

    let ((band_on, disagree_on), log_on) = run("stacked", Some("mock3"));
    let ((band_off, disagree_off), log_off) = run("single", None);
    eprintln!(
        "e2e stack ({exe}): stacked band={band_on} disagree={disagree_on} | \
         single band={band_off} disagree={disagree_off}"
    );

    // The envelope is one column per hour across 48 hours, so thousands of pixels.
    assert!(
        band_on > 1000,
        "the min-max spread envelope was not painted for the stacked mock ({band_on}):\n{log_on}"
    );
    // The chip is one line of small bold text; a few dozen solid pixels is a lot.
    assert!(
        disagree_on > 20,
        "the sources-disagree chip was not painted for the stacked mock ({disagree_on}):\n{log_on}"
    );
    assert_eq!(
        band_off, 0,
        "the spread envelope painted for a single-source run:\n{log_off}"
    );
    assert_eq!(
        disagree_off, 0,
        "the disagreement chip painted for a single-source run:\n{log_off}"
    );

    let _ = std::fs::remove_dir_all(&appdata);
}

/// The tray's "Refresh" (EXP22): the widget's real source is pointed at a
/// loopback server that answers the **first** request with `503` and the second
/// with the committed Open-Meteo fixture. `AIRQLALITY_TRAY_REFRESH_MS` fires the
/// same `commands::request(Command::Refresh)` the menu item does, so the widget
/// must fetch again immediately and render data - not the red "COULDN'T FETCH"
/// screen - without a restart and without waiting for the next `:00`/`:30`.
#[test]
fn widget_e2e_tray_refresh_recovers() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let png = std::env::temp_dir().join(format!(
        "airqlality-e2e-refresh-{}.png",
        std::process::id()
    ));
    let fetch_dump = std::env::temp_dir().join(format!(
        "airqlality-e2e-refresh-{}.jsonl",
        std::process::id()
    ));
    let tray_dump = std::env::temp_dir().join(format!(
        "airqlality-e2e-refresh-{}.json",
        std::process::id()
    ));
    // Isolate persistence: EXP24 made real keys a normal thing to have in
    // `%APPDATA%`, and those extra sources would join this loopback stack and
    // fail it. An empty APPDATA means Open-Meteo alone, as the seam expects.
    let appdata = std::env::temp_dir().join(format!(
        "airqlality-e2e-refresh-appdata-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");
    for path in [&png, &fetch_dump, &tray_dump] {
        let _ = std::fs::remove_file(path);
    }

    // A one-shot fake Open-Meteo: 503 first, the real fixture afterwards. The
    // widget's startup fetch is the first request by construction (the seam
    // fires seconds later), so no header inspection is needed.
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener.local_addr().expect("loopback address").port();
    let fixture = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("core")
            .join("tests")
            .join("fixtures")
            .join("openmeteo-cairo.json"),
    )
    .expect("read the committed Open-Meteo fixture");
    let hits = Arc::new(AtomicUsize::new(0));
    let served = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let request_no = served.fetch_add(1, Ordering::SeqCst) + 1;
            let mut head = [0u8; 4096];
            let _ = stream.read(&mut head);
            let (status, content_type, body) = if request_no == 1 {
                ("HTTP/1.1 503 Service Unavailable", "", String::new())
            } else {
                (
                    "HTTP/1.1 200 OK",
                    "Content-Type: application/json\r\n",
                    fixture.clone(),
                )
            };
            let response = format!(
                "{status}\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        // Real mode, but a loopback source that fails once and then recovers.
        .env("AIRQLALITY_OPENMETEO_URL", format!("http://127.0.0.1:{port}"))
        // The menu path, 3 s in; a long period keeps the boundary timer out of
        // the picture, so exactly two requests are expected.
        .env("AIRQLALITY_TRAY_REFRESH_MS", "3000")
        .env("AIRQLALITY_REFRESH_PERIOD_SECS", "7200")
        .env("AIRQLALITY_FETCH_DUMP", &fetch_dump)
        .env("AIRQLALITY_TRAY_DUMP", &tray_dump)
        .env("APPDATA", &appdata)
        .env_remove("AIRQLALITY_OPENAQ_KEY")
        .env_remove("AIRQLALITY_OWM_KEY")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");
    assert!(
        !stdout.contains("PROCESS_EXITED") && stdout.contains("SAVED"),
        "app did not render:\n{log}"
    );

    let requests = hits.load(Ordering::SeqCst);
    let rounds = std::fs::read_to_string(&fetch_dump).unwrap_or_default();
    let tray = std::fs::read_to_string(&tray_dump).unwrap_or_default();
    eprintln!(
        "e2e refresh: requests={requests}\n--- fetch rounds ---\n{rounds}--- tray ---\n{tray}"
    );

    assert!(
        requests >= 2,
        "the tray refresh never reached the source: {requests} request(s)\n{log}"
    );
    let lines: Vec<&str> = rounds.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        lines.len() >= 2,
        "expected a fetch round before and after the refresh:\n{rounds}{log}"
    );
    assert!(
        lines[0].contains("\"round\":1")
            && lines[0].contains("\"ok\":0")
            && lines[0].contains("Open-Meteo"),
        "the first round was supposed to fail on the 503:\n{rounds}"
    );
    assert!(
        lines[1].contains("\"round\":2")
            && lines[1].contains("\"ok\":1")
            && lines[1].contains("\"failed\":[]"),
        "the tray refresh did not bring data back as a second round:\n{rounds}"
    );
    // The tray leaves the muted "refreshing..."/"no data" state for a verdict.
    assert!(
        !tray.contains("\"color\":\"#8b949e\"") && tray.contains("Cairo"),
        "the tray icon never picked up the recovered verdict:\n{tray}"
    );

    // Pixels: the observed PM2.5 curve exists only in the data view.
    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());
    let mut curve = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] >= 200 && [px[0], px[1], px[2]] == PAST {
                curve += 1;
            }
        }
    }
    eprintln!("e2e refresh pixels ({iw}x{ih}): curve={curve}");
    assert!(
        curve > 200,
        "the widget still shows the error screen, not the recovered timeline (curve={curve}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&fetch_dump);
    let _ = std::fs::remove_file(&tray_dump);
    let _ = std::fs::remove_dir_all(&appdata);
}

/// EXP24: opening the key prompt must draw the first step over the widget (a
/// 404 px PANEL dialog) and must not write anything by merely opening it. The
/// seam stands in for the tray's "API keys..." item, which a posted key cannot
/// click.
#[test]
fn widget_e2e_keys_prompt_asks_for_openaq() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    // Isolate persistence from the developer's real %APPDATA%.
    let appdata = std::env::temp_dir().join(format!("airqlality-keys-1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-keys-1-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&png);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("7")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_KEYS_PROMPT", "1")
        .env("APPDATA", &appdata)
        .env_remove("AIRQLALITY_OPENAQ_KEY")
        .env_remove("AIRQLALITY_OWM_KEY")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");
    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let img = image::open(&png)
        .expect("decode screenshot PNG")
        .to_rgba8();
    let (iw, ih) = (img.width(), img.height());
    let mut panel = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] >= 200 && [px[0], px[1], px[2]] == PANEL {
                panel += 1;
            }
        }
    }
    eprintln!("e2e keys prompt pixels ({iw}x{ih}): panel={panel}");

    assert!(
        panel > 40_000,
        "the key dialog was not drawn (panel={panel}):\n{log}"
    );
    assert!(
        !appdata.join("AirQlality").join("config.json").exists(),
        "opening the prompt must not write a key by itself"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_dir_all(&appdata);
}

/// Count the dialog's panel pixels, the cheapest proof that a modal is on
/// screen (the closed widget has none).
fn panel_pixels(png: &Path) -> usize {
    let img = image::open(png).expect("decode screenshot PNG").to_rgba8();
    let (iw, ih) = (img.width(), img.height());
    let mut panel = 0usize;
    for y in 0..ih {
        for x in 0..iw {
            let px = img.get_pixel(x, y).0;
            if px[3] >= 200 && [px[0], px[1], px[2]] == PANEL {
                panel += 1;
            }
        }
    }
    panel
}

fn keys_dump(path: &Path, log: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("no keys dump ({e}):\n{log}"))
}

/// EXP24: typing a key and pressing Enter records it and **stays** on that
/// step - navigation is its own pair of buttons now. The screenshot shows the
/// dialog still open with the saved note; the file is the proof of the save.
#[test]
fn widget_e2e_keys_prompt_saves_and_stays() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let appdata = std::env::temp_dir().join(format!("airqlality-keys-2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-keys-2-{}.png", std::process::id()));
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-keys-2-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
    let key = "0123456789abcdef0123456789abcdef";

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("7")
        .arg("-Type")
        .arg(key)
        .arg("-Press")
        .arg("enter")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_KEYS_PROMPT", "1")
        .env("AIRQLALITY_KEYS_DUMP", &dump)
        .env("APPDATA", &appdata)
        .env_remove("AIRQLALITY_OPENAQ_KEY")
        .env_remove("AIRQLALITY_OWM_KEY")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");
    assert!(
        !stdout.contains("PROCESS_EXITED"),
        "app exited before it could render:\n{log}"
    );
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let config = appdata.join("AirQlality").join("config.json");
    let saved = std::fs::read_to_string(&config).unwrap_or_else(|e| {
        panic!("the Enter never saved a key ({e}):\n{log}");
    });
    eprintln!("e2e keys prompt saved: {saved}");
    assert!(
        saved.contains(&format!("\"openaq_key\":\"{key}\"")),
        "the typed OpenAQ key is not in the config:\n{saved}\n{log}"
    );
    assert!(
        !saved.contains("owm_key"),
        "saving one key must not touch the other slot:\n{saved}"
    );

    let state = keys_dump(&dump, &log);
    eprintln!("e2e keys state: {state}");
    assert!(
        state.contains("\"step\":1") && state.contains("\"saved\":[\"openaq\"]"),
        "save should stay on step 1 with OpenAQ stored:\n{state}\n{log}"
    );

    let panel = panel_pixels(&png);
    eprintln!("e2e keys saved pixels: panel={panel}");
    assert!(
        panel > 40_000,
        "the dialog closed on Save (panel={panel}):\n{log}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_dir_all(&appdata);
}

/// EXP24: Next is its own button - Tab off the field, Enter on Next - and it
/// must move to OpenWeatherMap without saving anything.
#[test]
fn widget_e2e_keys_prompt_navigates_to_owm() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let appdata = std::env::temp_dir().join(format!("airqlality-keys-nav-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-keys-nav-{}.png", std::process::id()));
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-keys-nav-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("7")
        // Field -> Next (Prev is disabled on step 1), Enter activates Next.
        .arg("-Press")
        .arg("tab enter")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_KEYS_PROMPT", "1")
        .env("AIRQLALITY_KEYS_DUMP", &dump)
        .env("APPDATA", &appdata)
        .env_remove("AIRQLALITY_OPENAQ_KEY")
        .env_remove("AIRQLALITY_OWM_KEY")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let state = keys_dump(&dump, &log);
    eprintln!("e2e keys nav state: {state}");
    assert!(
        state.contains("\"step\":2") && state.contains("\"saved\":[]"),
        "Next should show OpenWeatherMap and save nothing:\n{state}\n{log}"
    );
    assert!(
        !appdata.join("AirQlality").join("config.json").exists(),
        "navigating must not write a key"
    );
    assert!(
        panel_pixels(&png) > 40_000,
        "the dialog vanished while navigating:\n{log}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_dir_all(&appdata);
}

/// EXP24: the second prompt can be reached directly and saves into its own
/// slot, leaving OpenAQ alone.
#[test]
fn widget_e2e_keys_prompt_saves_owm_directly() {
    let _guard = window_guard();
    let exe = env!("CARGO_BIN_EXE_airqlality");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("capture.ps1");
    let appdata = std::env::temp_dir().join(format!("airqlality-keys-owm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&appdata);
    std::fs::create_dir_all(&appdata).expect("create temp APPDATA");
    let png = std::env::temp_dir().join(format!("airqlality-e2e-keys-owm-{}.png", std::process::id()));
    let dump = std::env::temp_dir().join(format!("airqlality-e2e-keys-owm-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
    let key = "b17ebb2f8a94df73e3633bc4e6fda481";

    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Exe")
        .arg(exe)
        .arg("-Out")
        .arg(&png)
        .arg("-WaitSeconds")
        .arg("7")
        .arg("-Type")
        .arg(key)
        .arg("-Press")
        .arg("enter")
        .env("AIRQLALITY_SOURCE", "mock")
        .env("AIRQLALITY_FREEZE_NOW", "1")
        .env("AIRQLALITY_KEYS_PROMPT", "2")
        .env("AIRQLALITY_KEYS_DUMP", &dump)
        .env("APPDATA", &appdata)
        .env_remove("AIRQLALITY_OPENAQ_KEY")
        .env_remove("AIRQLALITY_OWM_KEY")
        .output()
        .expect("failed to spawn capture.ps1");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let log = format!("--- capture stdout ---\n{stdout}\n--- capture stderr ---\n{stderr}");
    assert!(stdout.contains("SAVED"), "capture produced no screenshot:\n{log}");

    let config = appdata.join("AirQlality").join("config.json");
    let saved = std::fs::read_to_string(&config).unwrap_or_else(|e| {
        panic!("the Enter never saved the OWM key ({e}):\n{log}");
    });
    eprintln!("e2e keys owm saved: {saved}");
    assert!(
        saved.contains(&format!("\"owm_key\":\"{key}\"")),
        "the typed OpenWeatherMap key is not in the config:\n{saved}\n{log}"
    );
    assert!(
        !saved.contains("openaq_key"),
        "saving OpenWeatherMap must not touch OpenAQ:\n{saved}"
    );

    let state = keys_dump(&dump, &log);
    eprintln!("e2e keys owm state: {state}");
    assert!(
        state.contains("\"step\":2") && state.contains("\"saved\":[\"owm\"]"),
        "the second step should hold its own key:\n{state}\n{log}"
    );

    let _ = std::fs::remove_file(&png);
    let _ = std::fs::remove_file(&dump);
    let _ = std::fs::remove_dir_all(&appdata);
}
