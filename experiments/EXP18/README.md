# EXP18 - Align the real-source refresh to the wall clock (:00 / :30)

**Status:** `inProg` (started 2026-10-11).

## Summary
Make the real Open-Meteo refresh land on the wall-clock half hour (`:00` and
`:30`) instead of 30 minutes after each previous fetch, so a 12:22 launch ticks
at 12:30, 13:00, 13:30, ... rather than 12:52, 13:22, 13:52, ...

## Hypothesis
Sleeping `until_next_boundary(now, 30 min)` (the next wall-clock multiple of the
period) instead of a fixed `timer(REFRESH)` makes the first post-launch tick land
on `:00`/`:30` for any launch minute, keeps the initial fetch immediate, and
leaves every existing test (mock e2e, core, app) byte-identical.

**Falsified if:** a tick lands off `:00`/`:30`; the initial fetch is delayed; a
fetch at an exact boundary immediately fetches again; or any existing assertion
changes.

## Methodology & blast radius
- **Method**
  1. New pure module `app/src/schedule.rs` (no clock, no Win32, like `notify.rs`):
     `until_next_boundary(now_secs, period_secs) -> Duration` = `period - (now %
     period)`, returning a **full period** when `now` is exactly on a boundary
     (so a fetch that lands on the boundary does not immediately re-fetch).
     Unit-tested off-Windows with injected instants (`12:22 -> 8 min`,
     `12:00 -> 30 min`, `12:29:59 -> 1 s`).
  2. `ui.rs::spawn_real`: keep the immediate first `fetch()`, then sleep
     `until_next_boundary(SystemTime::now(), period)` instead of `timer(REFRESH)`.
     `REFRESH` stays 30 min; the loop re-reads the clock each iteration so it
     self-corrects (no accumulated drift).
  3. Test/demo seam `AIRQLALITY_REFRESH_PERIOD_SECS` overrides the period so the
     alignment can be watched live without waiting 30 min (ignored when 0/absent).
  4. Re-run `core`, `app` and the e2e suite.
- **May touch:** `app/src/schedule.rs` (new), `app/src/ui.rs`, `app/src/main.rs`,
  `experiments/EXP18/**`, `experiments-overview.md`, branch `EXP18` (from
  `171b411`, the EXP17 tip).
- **Off-limits:** `core/**` (the rule and `now_index` derivation are correct),
  `UI.md`, `AGENTS.md`, `experiments/EXP01..17/**` (frozen evidence), declaring a
  final status.
- Machine-level blast radius: none beyond the app's own request cadence; the mock
  3 s demo ticker and the mock e2e path are untouched, so pixel assertions cannot
  change.

## Open questions (known unknowns)
- [ ] Keep the 30 min cadence or go hourly? - **user chose: keep 30 min, aligned**
  to `:00`/`:30`. *(Decided.)*
- [ ] Alignment clock: device top-of-hour vs the location's offset? - proposed
  default: **device wall clock** (correct for Cairo and all whole-hour zones);
  half-hour-offset locations (e.g. Tehran `+03:30`) would roll at `:30`/`:00` of
  device time - a known cosmetic edge. *(Assumption adopted.)*
- [ ] Helper home: new `schedule.rs` vs inline? - proposed default: **new pure
  module**, consistent with `notify.rs`. *(Assumption adopted.)*
- [ ] Exact-boundary case: wait a full period rather than 0? - proposed default:
  **full period**, to avoid an immediate duplicate fetch. *(Assumption adopted.)*
- [ ] E2E proof of alignment? - proposed default: **no new e2e** (the mock path is
  a 3 s ticker); prove the arithmetic with unit tests and the seam. *(Assumption
  adopted.)*

## Conclusion (findings)
_Pending._

## Learnings
- The real refresh is anchored to launch time: `ui.rs::spawn_real` = `fetch()`
  then `timer(REFRESH)` (fixed 30 min from the previous fetch). The *visible*
  now-post only moves on the hour roll, so a 12:22 launch appears to update at
  13:22, 14:22 (hourly, launch-minute anchored).
- Align by sleeping to the next wall-clock multiple of the period, not a fixed
  duration: `app/src/schedule.rs::until_next_boundary(now_unix_secs, period)`,
  with a full period returned exactly on a boundary (no double fetch). Keep the
  first `fetch()` immediate; re-read `now` each loop so it self-corrects.
- The mock e2e path uses the separate 3 s `TICK` and never calls `spawn_real`,
  so wall-clock alignment cannot perturb the pixel assertions.
- `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;" + $env:PATH; cargo test
  -p airqlality --bin airqlality` (app units), `-p airqlality-core`, and
  `-p airqlality --test e2e_widget` (22/22, ~3 min) - use for the full gate.
  The only build warning is the pre-existing `.rsrc merge failure` from the
  EXP14 `windres` icon; harmless.
- `cargo build --release -p airqlality` fails with `failed to remove file
  ...\target\release\airqlality.exe (Access is denied)` while the app is
  running: stop it first (`Get-Process airqlality | Stop-Process`) then rebuild,
  and relaunch afterwards. Location survives (persisted to `%APPDATA%`).
