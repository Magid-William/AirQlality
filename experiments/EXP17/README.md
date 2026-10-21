# EXP17 - System notification only when the window should be closed

**Status:** `inProg` (started 2026-10-05).

## Summary
Warn the user with a Windows system notification when - and only when - the air
turns bad enough that the room window should stay shut (`KEEP CLOSED`). Never
for `OPEN`/`VENT`, and never repeatedly while it stays closed.

## Hypothesis
An **edge-triggered** notification wired to the existing tray icon
(`Shell_NotifyIconW` + `NIF_INFO`/`szInfo`/`szInfoTitle`, `NIIF_WARNING`) fires
exactly once each time the headline verdict *transitions into* `KeepClosed`
(including a cold start that is already closed), and zero times for `Open`/`Vent`
or repeated same-verdict evaluations - while every existing e2e/pixel/tray/
startup/a11y assertion stays byte-identical.

**Falsified if:** a notification fires for a non-`KeepClosed` verdict; it fires
more than once for one closed spell; the shell rejects the call; or any existing
assertion changes.

## Methodology & blast radius
- **Method**
  1. New pure module `app/src/notify.rs` (no Win32, compiled everywhere):
     `should_notify(previous: Option<Verdict>, current: Verdict) -> bool` fires
     only on `current == KeepClosed && previous != KeepClosed`; `message(...)`
     builds the title/body. Unit-tested off-Windows.
  2. Wire into `app/src/tray.rs`: `set_verdict` runs the gate and, on fire, stores
     a **pending info** payload in the shared `STATE`; `add_icon` and
     `refresh_icon` flush it with `NIM_MODIFY` + `NIF_INFO`. Pending (not
     immediate) because `Widget::new` evaluates the mock verdict *before*
     `tray::start` creates the icon, so the cold-start warning must not be lost.
     `set_loading` resets the last verdict, so switching to a closed location is
     a fresh spell.
  3. Test seam `AIRQLALITY_NOTIFY_DUMP=<path>`: append one JSON line per balloon
     actually sent (`verdict`, `ok`, title/body), separate from the existing
     `AIRQLALITY_TRAY_DUMP`.
  4. E2E (black-box, Win32 boundary): frozen mock "now"=23 (`26 ug/m3` ->
     `KEEP CLOSED`) -> exactly one dump line with `ok:true`; mock pinned to index
     36 (`OPEN`) -> no notification. Re-run the full suite.
- **May touch:** `app/src/notify.rs` (new), `app/src/tray.rs`, `app/src/main.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP17/**`, `experiments-overview.md`,
  branch `EXP17` (from `2787774`).
- **Off-limits:** `core/**` (the rule is unchanged), `UI.md`, `AGENTS.md`,
  `experiments/EXP01..16/**` (frozen evidence), declaring a final status.
- Machine-level blast radius: the shell may show a balloon on a closed verdict;
  the app writes only to the E2E temp dump path. No registry, no new crate.

## Open questions (known unknowns)
- [ ] Fire on every evaluation while closed, or edge-only? - proposed default:
  **edge-only (one per closed spell)**, so the 30 min refresh does not re-nag and
  the accelerated demo ticker does not spam. *(Assumption adopted.)*
- [ ] Notify on a cold start already closed? - proposed default: **yes** - a
  launch into bad air is the main use case. *(Assumption adopted.)*
- [ ] `Shell_NotifyIconW` `NIF_INFO` balloon vs a WinRT toast? - proposed
  default: **`NIF_INFO`** - reuses the EXP11 tray, no new crate; on Win10/11 it
  renders as a system notification. *(Assumption adopted.)*
- [ ] Severity/sound? - proposed default: **`NIIF_WARNING`, default sound,
  `NIIF_RESPECT_QUIET_TIME`**. *(Assumption adopted.)*
- [ ] Suppress while our own widget is visible/focused? - proposed default:
  **no**; the verdict is about the room, independent of our window. *(Assumption
  adopted.)*
- [ ] Prove the on-screen balloon in E2E? - proposed default: **no pixel
  assertion** (shell-mediated, suppressed by Focus Assist / Do Not Disturb);
  assert the gate fired and the shell accepted the call. *(Assumption adopted.)*
- [ ] Tray-menu on/off toggle? - proposed default: **none this experiment**
  (candidate follow-up). *(Assumption adopted.)*

## Conclusion (findings)
_Pending._

## Learnings
- A Windows balloon from an **existing** tray icon is one extra `Shell_NotifyIconW(NIM_MODIFY)` with `uFlags = NIF_INFO`, `szInfo`/`szInfoTitle`, and
  `dwInfoFlags = NIIF_WARNING | NIIF_RESPECT_QUIET_TIME`. `NIF_INFO`,
  `NIIF_WARNING`, `NIIF_RESPECT_QUIET_TIME` all live in the already-enabled
  `Win32_UI_Shell` feature, so no `Cargo.toml`/feature change is needed. `ok`
  comes back `true` even when Focus Assist suppresses the visible banner.
- The first verdict is evaluated in `Widget::new`, which runs inside
  `open_window` **before** `tray::start` creates the icon. A notification fired
  immediately would be lost, so queue it (`State.pending`) and flush it from
  `add_icon`/`refresh_icon`; `set_loading` clears the queue and the last verdict
  so a location switch is a fresh spell.
- Edge-trigger the warning on `previous != KeepClosed && current == KeepClosed`
  (a pure fn in `notify.rs`), never on every evaluation - otherwise the 30 min
  refresh and the 3 s demo ticker re-nag.
- Black-box notification test seam: append one JSON line per balloon actually
  sent to `AIRQLALITY_NOTIFY_DUMP`; a non-`KeepClosed` run leaves the file
  absent. Matches the existing `AIRQLALITY_TRAY_DUMP` approach.
- `cargo test -p airqlality --test e2e_widget -- <filter>`: a substring filter
  must match the test name, so `notify` misses `notifies_*` - use `notif`.
- `cargo build`/`test` needs the MinGW toolchain on PATH:
  `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;" + $env:PATH`.
  A fresh worktree has no `target/`, so the first build of `gpui` is ~6-7 min.
- The linker emits a pre-existing warning (`.rsrc merge failure: multiple
  non-default manifests`) from the EXP14 `windres` icon; harmless.
