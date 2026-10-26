# EXP30 - Warn on the data's clock, not the fetch clock

**Status:** `inProg` (started 2026-10-26).

## Summary
Every time-dependent output is a snapshot of the last request, so an event at
16:00 can be announced at 16:30. Re-derive "now" from the series' timestamps
every minute and plan the warning 10 min before the next transition.

## Hypothesis
The app derives its now-post, verdict, tray icon and warning only when a fetch
completes (`ui.rs::spawn_real` -> `apply_stack` -> `set_verdict`); real mode has
no other clock. Therefore, if the evaluation instant is re-anchored from the
cached series' own timestamps on a wall-clock tick, and one balloon is planned
`lead` before the next achieved transition (`lead` = 10 min), then:

- the balloon lands at `T - lead` +/- 1 min whenever the forecast was known in
  advance, **independent of the 30 min fetch cadence, a failed request, or a
  slow one**;
- the now-post, verdict and tray icon flip at `T`, also without a fetch;
- no second balloon fires at `T`, and one balloon per run still holds, as does
  silence for the verdicts the user did not choose (EXP17, EXP27, EXP28).

**Falsified if:** a normal run's balloon is more than one minute away from
`T - lead`; a second balloon fires when the run starts; the icon/now-post needs
a request to move; or an existing notification contract changes.

## Methodology & blast radius

The data is hourly, not 10-minutely: the live Air Quality API reports
`current.interval = 3600`, and `minutely_15=pm2_5` returns no series. So the
"in 10 minutes" event is the hourly forecast boundary, and the only honest way
to warn before it is a lead on a transition we already hold - not finer polling.

- **Method**
  1. `app/src/clock.rs` (new, pure): `until_next_tick` (aligned to the minute),
     plus the evaluation-only simulated clock seam
     `AIRQLALITY_CLOCK_EPOCH` / `AIRQLALITY_CLOCK_SPEED`. The fetch schedule
     keeps real time, so the two clocks cannot fight.
  2. `app/src/onset.rs` (new, pure): `Planner::poll(series, decision, now,
     targets, lead)` finds the next achieved transition, emits a `Warning` when
     `now >= start - lead` (or a cold start / wake-up already inside a run),
     and de-duplicates by the absolute hour range of the run. The old
     `notify::Gate` retires; its contract sequences move here.
  3. `app/src/ui.rs`: one `reanchor(now)` path - re-indexes the fused series and
     every source from the data's timestamps, re-evaluates, updates the tray
     icon and asks the planner. Called by `apply_stack`, the mock tick and a new
     1 min evaluation task. No network, no new crate.
  4. `app/src/tray.rs`: `set_verdict` becomes icon-only; new `warn(..)` sends the
     balloon. The balloon dump gains `at` (the evaluation instant) and `kind`
     (`ahead` / `now` / `late`); `hour` is the run's first hour, so the EXP27/28
     assertions keep their meaning.
  5. `app/src/notify.rs`: the body keeps `Cairo, Egypt · PM2.5 30 µg/m³` as its
     prefix and appends ` · from 4:00 PM (in 10 min)`, so no existing body
     assertion changes.
  6. Windowed E2E (real path, loopback source): a six-hour JSON (15:00 clean,
     16:00 = 30), clock pinned to 15:45 at 120x, a week's refresh period,
     isolated `%APPDATA%`. Assert one balloon at sim 15:50, still green at
     15:5x, red by 16:0x, and exactly one fetch round. Bite-proof: commenting
     out `start_eval_clock` makes the ahead test fail with **0** balloons.
  7. Re-run core, app and the full windowed E2E once at the end.
- **Evidence:** `EXP30-open.png` (the widget still reads OPEN at 15:5x while the
  balloon has already gone out), `EXP30-ahead.png` (16:0x, red, still one
  balloon), and the dumps: `at=2026-01-01T15:50:00+02:00`, `kind=ahead`,
  `from 4:00 PM (in 10 min)`, `hour=490910` (the 16:00 run), tray `#3fb950`
  then `#f85149`, and exactly **one** fetch round. Suites: core 64/64, app
  86/86, windowed E2E 35/35 in 295.91 s.
- **May touch:** `app/src/ui.rs`, `app/src/tray.rs`, `app/src/notify.rs`, new
  `app/src/clock.rs`, new `app/src/onset.rs`, `app/src/main.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP30/**`, `experiments-overview.md`.
- **Off-limits:** `core/**` (the rule, fusion and `now_index_for` are correct),
  `UI.md`, `AGENTS.md`, EXP01..EXP29, declaring a final status.
- **Machine-level:** one wake per minute while the widget runs; no new network
  calls, no new crates; ~2 new windowed tests (~30 s).

## Open questions (known unknowns)

- [ ] Lead time? - proposed default: **10 min** (the user's own example), from
  `AIRQLALITY_NOTIFY_LEAD_SECS`, tray-configurable later. *(Assumption adopted:
  the constant is a parameter, so 0 would mean exact-boundary only.)*
- [ ] Lead + onset, or lead only? - proposed default: **one balloon, the lead
  one**; a run first seen after its lead window (cold start, wake, revision)
  warns once immediately as "since 4:00 PM".
- [ ] Event instant: hour boundary or the interpolated 25 ug/m3 crossing on the
  curve? - proposed default: **hour boundary**, the data's own granularity; the
  message names the clock time.
- [ ] Forecast revises after the balloon was sent? - proposed default: **no
  retraction, no second balloon for the same run**; icon and text follow the
  revision. Noted as a limitation.
- [ ] Sleep/wake? - proposed default: **warn once for the current run however
  late**, as "since HH:MM"; no silent misses.
- [ ] Who owns the de-duplication? - proposed default: **the planner**. Pre-
  observing a future hour inside the old `Gate` corrupts its monotonic hour
  state; the tray only transports.
- [ ] Tick granularity? - proposed default: **60 s aligned to :00** (<= 1 min
  error, negligible battery).
- [ ] Banner line "closes 4:00 PM (in 10 min)"? - proposed default: **no layout
  change this experiment**; the ribbon already shows the future.
- [ ] Keep the 30 min fetch cadence? - proposed default: **yes**, unchanged
  (EXP18 stays as recorded; EXP30 only stops depending on it).

## Conclusion (findings)
_Pending._

## Learnings
- The real path had no clock: `spawn_real` fetches, then sleeps to the next
  `:00`/`:30` (`app/src/schedule.rs`), and only `apply_stack` wrote the series,
  decision, tray icon and balloon. The 3 s ticker is mock-only.
- Open-Meteo's air-quality data is hourly (`current.interval = 3600`); a
  `minutely_15=pm2_5` request returns no series, so there is no finer event.
- `Instant::elapsed().as_secs()` truncates: at 120x a 500 ms sleep is a
  simulated minute, but whole-second truncation froze the clock between real
  seconds, so ticks landed at 15:45, 15:47, ... and the lead came out a minute
  late. Use `as_millis()` in any simulated clock.
- De-duplication is a **range** question, not a state machine: one warning per
  `(verdict, [start_hour, end_hour])` run, with overlap meaning "same spell".
  A redrawn forecast that extends or shifts the same spell overlaps and stays
  silent; the demo ticker's replay does too, because the absolute hours repeat.
  This retired `notify::Gate`; its six-hour contract tests moved to `onset`.
- The tray no longer gates: `set_verdict` is icon-only and `warn` transports
  what `onset::Planner` decided. `notify_targets()` is read from the tray's
  live state once per tick, so a "Notify me on" toggle still applies without a
  restart.
- `$env:PATH = "$env:LOCALAPPDATA\mingw64\bin;" + $env:PATH; cargo test -p
  airqlality --bin airqlality` (86/86), `-p airqlality-core --lib` (64/64),
  `-p airqlality --test e2e_widget` (35/35, 296 s).
- `cargo test -p airqlality --test e2e_widget -- --nocapture widget_e2e_warns`
  runs only the two EXP30 tests (~22 s) and prints the dumps.
- Recording evidence: set `AIRQLALITY_EVIDENCE_DIR=experiments/EXP30` for the
  run; `run_onset` then copies `EXP30-{ahead,open}.{png,jsonl,json}` there.
- Bite-proof: with `widget.start_eval_clock(cx)` commented out, the same test
  renders the same window but writes **0** notify lines (the 16:00 value is
  never reached: the only state is the fetch's 15:00 snapshot). The eval tick
  is the whole fix; the rest is plumbing.
