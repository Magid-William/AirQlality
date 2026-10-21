# EXP28 - "Notify me on" submenu: closing, opening, venting

**Status:** `inProg` (started 2026-10-22).

## Summary
A tray submenu with three independent checks - closing / opening / venting the
window - persisted across restarts. The default is unchanged (closing only), and
each enabled verdict warns once per run of that verdict.

## Hypothesis
The three choices live in `%APPDATA%\AirQlality\notify.txt` and the EXP27 gate
reads them, so:

- each **enabled** verdict warns once per contiguous run of that verdict
  (`[green][orange][red][orange][red][orange]` gives 2 with closing only, 6 with
  all three);
- a **disabled** verdict never warns, and nothing checked means silence;
- the choice survives a restart (cold start already closed still honours it);
- the submenu's native checkmarks always agree with what actually notifies.

**Falsified if:** a disabled verdict notifies, an enabled one warns twice in one
run, a same-hour flicker double-fires, the setting is lost on restart, or the
checkmarks disagree with the gate.

## Methodology & blast radius
- **Method**
  1. `app/src/notify.rs`: pure `Targets { closed, vent, open }` with
     `CLOSED_ONLY` default, `parse`/`to_text` (`closed,vent,open` / `none`),
     `toggle`, `allows`; the `Gate` remembers the last `(verdict, hour)` it
     warned for and fires for any enabled verdict once per run. `message` gains
     the verdict: `KEEP THE WINDOW CLOSED` / `OPEN THE WINDOW` /
     `VENTILATE BRIEFLY`.
  2. `app/src/persist.rs`: `load_notify`/`save_notify` on a separate
     `notify.txt` (no lost-update race with the `config.json` key writes);
     `AIRQLALITY_NOTIFY_ON` overrides the file for tests.
  3. `app/src/tray.rs`: lazy-loaded targets in `State` so the cold-start warning
     honours them; a real submenu (`CreatePopupMenu` + `MF_POPUP`) under
     `Notify me on`; toggling updates the state and saves immediately, so it
     works with the widget hidden; balloon severity follows the verdict
     (warning for closed, info for vent/open); the dump names the verdict.
  4. Tests: `Targets` parse/serialize; the gate per target set (2 / 6 / 1 /
     silence, same-hour flicker); persist round-trip and fallbacks; the real
     submenu read back with `GetMenuState`; windowed E2E with an isolated
     `%APPDATA%` (`closed,vent` -> 2 balloons, `none` -> none).
  5. Evidence: `experiments/EXP28/capture-menu.ps1` (adapted from EXP22's real
     right-click) screenshots the open submenu with its checks.
- **Evidence:** `EXP28-tray-menu.png` (the open submenu: closed and venting
  checked, opening not) and `EXP28-open-balloon.png` (`OPEN THE WINDOW`, info
  icon). Suite: core 64/64, app 82/82 (+9 `notify`, +2 `tray`), windowed E2E
  33/33 (+2).
- **May touch:** `app/src/notify.rs`, `app/src/tray.rs`, `app/src/persist.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP28/**`,
  `experiments-overview.md`, branch `EXP28` (from `EXP27`).
- **Off-limits:** `core/**` (rule and thresholds untouched), `UI.md`,
  `AGENTS.md`, `experiments/EXP01..27/**`, declaring a final status.

## Open questions (known unknowns)
- [ ] Default selection? - **closing only** (existing installs unchanged).
  *(Assumption adopted.)*
- [ ] Is "nothing checked" allowed? - **yes**, it means silence. *(Assumption
  adopted.)*
- [ ] Menu wording? - `Notify me on` > `Closing the window`, `Opening the
  window`, `Venting the window`, after `API keys...`. *(Assumption adopted.)*
- [ ] Balloon titles? - closed keeps the existing imperative; open
  `OPEN THE WINDOW`; vent `VENTILATE BRIEFLY` (the in-app advice says short
  bursts only). *(Assumption adopted.)*
- [ ] Severity? - closed `NIIF_WARNING`, vent/open `NIIF_INFO`. *(Assumption
  adopted.)*
- [ ] All three on: per segment type (6 for the six-step pattern)? - **yes**,
  the same once-per-run rule. *(Assumption adopted.)*
- [ ] Storage: separate `notify.txt` vs `config.json`? - **`notify.txt`**.
  *(Assumption adopted.)*
- [ ] Confirmation balloon on toggle? - **no**; the checkmark is the feedback.
  *(Assumption adopted.)*

## Conclusion (findings)
_Pending._

## Learnings
- A native tray submenu is `CreatePopupMenu` + one `AppendMenuW` per check + 
  `AppendMenuW(main, MF_POPUP, submenu.0 as usize, label)`; the selected leaf id
  comes back from `TrackPopupMenu(TPM_RETURNCMD)`, and `DestroyMenu(main)`
  disposes the children. No new crate or feature.
- The checkmarks can be proved without a GUI: build the **real** submenu with
  `notify_submenu()` and read each item back with
  `GetMenuState(submenu, id, MF_BYCOMMAND)`; `u32::MAX` means the item is
  missing, and `state & MF_CHECKED.0` is the check.
- The tray icon's slot moves between runs. Locate it by its verdict colour
  (exact `#3fb950`/`#f85149`) in a ~500x60 strip of the bottom-right corner and
  right-click its centre; `SendKeys "{DOWN}x4 {RIGHT}"` opens `Notify me on`
  for the evidence shot. Pin the mock to an OPEN hour so no balloon covers the
  corner.
- A separate `notify.txt` avoids a lost update: `save_keys` rewrites
  `config.json` wholesale from its own thread, so two writers would fight.
- `set_verdict` runs before `tray::start` (EXP17), so `State.targets` must load
  lazily on first use; a `Mutex::new` static also needs a `const fn`
  constructor (`Targets::CLOSED_ONLY`, `Gate::new`).
- Unit tests must pin `STATE.targets` and the older notify E2E tests set
  `AIRQLALITY_NOTIFY_ON=closed`, or the suite reads the developer's own
  `notify.txt` and fails where the user's choices differ.
- `cargo test -p airqlality --test e2e_widget -- follows_the_saved_choices` and
  `-- notify_none`: the two EXP28 runs, ~8 s and ~6 s.
