# EXP27 - One notification per red segment, never per refresh

**Status:** `inProg` (started 2026-10-21).

## Summary
Prove and enforce the warning contract: a `KEEP CLOSED` run warns once -
`[green][orange][red][orange][red][orange]` gives 2, however long each red
lasts - and a refresh that flickers red/orange/red inside one hour adds none.

## Hypothesis
The notification gate is keyed on **hour buckets**: it fires at most once per
contiguous `KeepClosed` run of hourly verdicts, re-arming only after a whole
non-red hour that is strictly later than the warning hour.

- `[Open][Vent][Closed][Vent][Closed][Vent]` -> exactly **2** warnings.
- `[Open][Vent][Closed][Closed][Closed][Closed]` -> exactly **1**, no matter
  how long the closed run is.
- `[Closed]` (hour 2), `[Vent]` (hour 2), `[Closed]` (hour 2) -> **1** - today
  this fires twice, so the gate re-arms on a single non-red evaluation.
- A cold start that is already closed -> 1; a run separated by a full non-red
  hour -> warns again; `Open`/`Vent` never warn.

**Falsified if:** one closed run produces two warnings, or a genuinely new run
(or a cold start into bad air) stays silent.

## Methodology & blast radius
- **Method**
  1. `app/src/notify.rs`: a pure `Gate` (hour of the last warning + the last
     non-red hour) replaces `should_notify(previous, current)`; tests operate on
     verdict/hour sequences, including the exact six-step pattern above.
  2. `app/src/tray.rs`: `State.gate` replaces `last_verdict`; `set_verdict`
     takes the hour of the reading; `set_loading` resets the gate (new place =
     new run), `set_refreshing` keeps it (EXP22). The notify dump gains `hour`.
  3. `app/src/ui.rs`: the three `set_verdict` call sites pass
     `series.now().time.timestamp() / 3600`; a non-red hour must be *observed*,
     not inferred.
  4. Test seams: `AIRQLALITY_MOCK_PM25` (a comma list -> a synthetic hourly
     series, `now = 0`) and `AIRQLALITY_TICK_MS` (accelerated demo ticker), so
     the windowed E2E can walk a crafted pattern through the real app.
  5. Windowed E2E at the Win32 boundary: the six-step pattern -> exactly 2
     `AIRQLALITY_NOTIFY_DUMP` lines; a long closed run -> exactly 1, both `ok`.
- **Evidence:** `EXP27-segments.png` + `EXP27-segments.jsonl` (two balloons, three
  hours apart, PM2.5 30 and 35) and `EXP27-long-run.png` +
  `EXP27-long-run.jsonl` (six closed hours, one balloon). Suite: core 64/64,
  app 73/73 (+6 `notify::Gate`), windowed E2E 31/31 (+2).
- **May touch:** `app/src/notify.rs`, `app/src/tray.rs`, `app/src/ui.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP27/**`,
  `experiments-overview.md`, branch `EXP27`.
- **Off-limits:** `core/**` (thresholds and the rule are unchanged), `UI.md`,
  `AGENTS.md`, `experiments/EXP01..26/**`, declaring a final status.

EXP17 stays as recorded: the balloon mechanism and its learnings stand. EXP27
supersedes only **when the gate re-arms**.

## Open questions (known unknowns)
- [ ] Segment = hour bucket, not "the verdict never flapped"? - proposed
  default: **hour bucket**, because the timeline and the data are hourly.
  *(Assumption adopted.)*
- [ ] Same-hour flicker (red -> orange -> red)? - proposed default: **silent**,
  one run. *(Assumption adopted.)*
- [ ] Warn again after an app restart mid-run? - proposed default: **yes**, a
  new session warns once; persisting the hour is out of scope. *(Assumption
  adopted.)*
- [ ] App launched mid-run? - proposed default: **warn once** (the main use
  case). *(Assumption adopted.)*
- [ ] Location switch resets the gate? - proposed default: **yes**, a different
  place is a different run. *(Assumption adopted.)*
- [ ] Tray toggle / quiet hours? - proposed default: **none** this experiment.
  *(Assumption adopted.)*
- [ ] Mock seam shape? - proposed default: **`AIRQLALITY_MOCK_PM25` +
  `AIRQLALITY_TICK_MS`**, env-only, no fixture format. *(Assumption adopted.)*

## Conclusion (findings)
_Pending._

## Learnings
- The EXP17 gate could still warn twice for **one** red segment: a refresh that
  re-evaluated the same hour as `Vent` and then `KeepClosed` re-armed it. Keying
  the gate on the **hour bucket of the reading** (`series.now().time.timestamp()
  / 3600`, not the wall clock) fixes that; re-arming then needs a non-red hour
  strictly later than both the warning hour and the clean hour.
- A same-hour check is enough for durability: `cleared > warned && hour >
  cleared` keeps the demo ticker's `last -> 0` wrap from counting as a new
  segment, which the frozen EXP17 `mock_cycle_warns_once_per_closed_spell`
  contract still pins at 3.
- `AIRQLALITY_MOCK_PM25` + `AIRQLALITY_TICK_MS` let the windowed E2E drive an
  exact verdict pattern through the real app (`12,20,30,30,20,35,20` -> 2
  balloons, `...30,30,30,30...` -> 1). Pad the list with clean hours: the
  ticker wraps at the end and would otherwise re-play the pattern into a second
  segment.
- `cargo test -p airqlality --test e2e_widget -- per_red_segment` /
  `-- long_red_run`: substring filters against the test name, ~10 s and ~6 s.
