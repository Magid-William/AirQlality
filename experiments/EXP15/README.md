# EXP15 - Suppress the console window (Windows GUI subsystem)

**Status:** `inProg` (started 2026-10-05).

## Summary
The widget is a GUI app but ships as a Windows console binary, so Windows opens an empty terminal behind it. Switch the executable to the GUI subsystem so no console is allocated.

## Hypothesis
Adding `#![windows_subsystem = "windows"]` to the binary crate root (`app/src/main.rs`) flips the built `.exe`'s PE `Subsystem` field from `3` (Windows CUI) to `2` (Windows GUI), so launching the app allocates no console window - while the window, tray, close-to-tray, `--startup` hidden launch, accessibility tree, and every existing e2e pixel count stay byte-identical.

**Falsified if:** the PE `Subsystem` stays `3`; the exe still gets a console; or any existing e2e / tray / startup assertion changes.

## Methodology & blast radius
- **Method**
  1. `app/src/main.rs`: add `#![windows_subsystem = "windows"]` at the crate root. The attribute only affects the *binary* target; test harnesses keep their own console subsystem and their `eprintln!` output is unaffected.
  2. Verify black-box: a new e2e case parses the PE header of the built `.exe`
     (`e_lfanew` -> optional header -> `Subsystem` u16 at optional-header offset `68`) and asserts `2` (GUI) not `3` (console). Pure Rust, no MSVC/`dumpbin`, no new crate.
  3. Run the full suite to confirm the existing assertions are unchanged
     (`closed=2043 vent=5600 open=790 panel=0`, tray/tooltip/a11y/startup/icon).
- **May touch**: `app/src/main.rs` (one attribute), `app/tests/e2e_widget.rs`
  (one new test), `experiments/EXP15/**`, `experiments-overview.md`, branch
  `EXP15` (from `c53f6a8`).
- **Off-limits**: `core/**`, `UI.md`, `AGENTS.md`, `experiments/EXP01..14/**`
  (frozen evidence), declaring a final status.
- Machine-level blast radius: the process allocates no console. Nothing is written outside the repo.

## Open questions (known unknowns)
- [ ] Hide the console in **debug builds too**, or only release? - proposed default: **both (unconditional attribute)**. The app writes nothing to stdout in any runtime path (all `println!`/`eprintln!` live in `build.rs` and the test harness), so there is no debug logging to lose. *(Assumption adopted.)*
- [ ] Assert via **PE header** or a runtime `GetConsoleWindow` seam? - proposed default: **PE header** - deterministic and needs no seam in the app. *(Assumption adopted.)*

## Conclusion (findings)
_Pending._

## Learnings
- `#![windows_subsystem = "windows"]` at the binary crate root flips the exe's
  PE `Subsystem` from `3` (console) to `2` (GUI), so Windows allocates no
  console. Place it after the `//!` doc block and before the first `mod`. It
  affects only the binary target; test harnesses keep their own console.
- Read the PE subsystem in a test with pure `std`: `e_lfanew` is a u32 at file
  offset `0x3C`, signature `PE\0\0` follows, optional header at
  `e_lfanew + 24`, and `Subsystem` is a u16 at optional-header offset `68`
  (same offset for PE32 and PE32+). Assert `2`.
- Runtime proof of "no console": `EnumWindows` + `GetWindowThreadProcessId`
  filtered to the app pid; a console would be a `ConsoleWindowClass` top-level
  window. The widget's own windows are `Zed::Window` (gpui) and
  `AirQlalityTrayWindow` (hidden).
- `cargo build`/`test` needs the MinGW toolchain on PATH:
  `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;" + $env:PATH`.
- A running release exe locks `target\release\airqlality.exe` (build fails with
  "Access is denied (os error 5)"); `Stop-Process` it first.
- E2E flake (pre-existing, not this change): `widget_e2e_shows_fetch_error` can
  paint no red if the unreachable-endpoint fetch outlives the 6 s capture wait
  under load. It passes in isolation; confirm before blaming a change.

