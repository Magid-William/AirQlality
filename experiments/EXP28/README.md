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
**Success (user, 2026-10-22).** The tray now has `Notify me on >` with three
independent checks - `Closing the window` (default, checked), `Opening the
window`, `Venting the window` - and the checked set is what the gate promises.

What landed:
- `notify::Targets` (pure) is the single source of truth: parse/serialize,
  `allows`, `toggle`; `%APPDATA%\AirQlality\notify.txt` stores it as
  `closed,open,vent` or `none`, with `AIRQLALITY_NOTIFY_ON` winning for a quick
  try. A missing file is the pre-EXP28 default, so existing installs are
  unchanged; `none` is an explicit silence.
- `Gate` is generalized from one verdict to any enabled one, keeping EXP27's
  hour-bucket rule: one balloon per run of each enabled verdict, a whole other
  hour between runs to re-arm, same-hour flicker silent, and the demo wrap not a
  new run. Closing only still gives 2 for `[green][orange][red][orange][red]
  [orange]`; all three give 6 (one per segment type), which is the documented
  consequence of asking about every band.
- The submenu is a real `MF_POPUP` built from the stored choices on every open,
  so the checkmarks cannot disagree with the gate; toggling flips the state,
  saves, and takes effect immediately with the widget hidden.
- Balloons gained a per-verdict title (`KEEP THE WINDOW CLOSED`, `OPEN THE
  WINDOW`, `VENTILATE BRIEFLY`) and severity (warning for closed, info for the
  optional two). The dump names the verdict and hour.

Evidence: `EXP28-tray-menu.png` (submenu open, closing + venting checked,
opening not), `EXP28-open-balloon.png` (the info-icon `OPEN THE WINDOW` toast),
and the E2E dumps (`closed,vent` -> 2, `none` -> 0). Suite: core 64/64, app
82/82 (+9 `notify`, +2 `tray`), windowed E2E 33/33 in 291 s.

Limits / not proven:
- The menu *click* itself is not automated. The submenu the shipping code builds
  is read back with `GetMenuState`, and the saved file drives the E2E, so the
  chain is covered - but no test presses the item. The screenshot is a real
  right-click.
- The choice is per machine and per session; a restart mid-run warns once, and
  an `AIRQLALITY_NOTIFY_ON` override makes menu toggles look ignored (same
  precedence as the API keys).
- A green/vent notification is only as good as the fused, hourly data; the
  once-per-run rule means a band that flips twice within one hour is reported
  once.

## Learnings
- Native tray submenu: `CreatePopupMenu`, one `AppendMenuW` per check, then
  `AppendMenuW(main, MF_POPUP, submenu.0 as usize, label)`; `TrackPopupMenu(
  TPM_RETURNCMD)` returns the leaf id and `DestroyMenu(main)` disposes children.
- Prove the checks without a GUI: build the real submenu with `notify_submenu`
  and read each item with `GetMenuState(submenu, id, MF_BYCOMMAND)` (`u32::MAX`
  = missing, `state & MF_CHECKED.0` = checked).
- Keep the choice in its own `notify.txt`: `save_keys` rewrites `config.json`
  wholesale, so two writers would lose updates. Load it lazily in `State` -
  `set_verdict` runs before `tray::start` - and pin it in tests, or the suite
  reads the developer's own file.
- The tray slot moves: find the icon by its verdict colour in a ~500x60
  bottom-right strip, right-click its centre, then `SendKeys "{DOWN}x4 {RIGHT}"`
  to open the submenu. Pin the mock to an OPEN hour so no balloon covers it
  (`experiments/EXP28/capture-menu.ps1`).
