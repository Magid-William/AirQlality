# EXP07 - Location dropdown (Cairo, Egypt default)

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Turn the fixed "Cairo, Egypt" header label into a clickable dropdown of a curated
location list. Selecting one re-points `OpenMeteoSource` at that location's
lat/lon/timezone, refetches, and updates the header. Cairo, Egypt is the default;
the last choice is remembered across restarts. The panel is virtualized with
`uniform_list` so the row count has no ceiling.

## Hypothesis
A pointer-only, in-window `uniform_list` dropdown (no native popup, no new deps)
can switch the widget between locations: the selection drives `OpenMeteoSource`'s
coordinates/timezone, the header follows the selection, the last choice persists
across restarts, and the mock path stays byte-identical so the EXP04/05/06 E2E
verdict counts are unchanged.

**Falsified if:** gpui-ce can't render/scroll a `uniform_list` inside an overlay,
the source can't be re-pointed without blocking the UI thread or leaking the old
30-min refresh loop, or the default state can't keep the existing E2E contract.

## Methodology & blast radius
- Method:
  1. **`core` - model the location.** New `core/src/location.rs`: `Location
     { id, label, lat, lon, tz }` + `pub const LOCATIONS` (Cairo first). Add
     `OpenMeteoSource::from_location(&Location)` (keep `cairo()` as a wrapper) and
     thread the label into `Series` (a `location: &str` arg on `parse_openmeteo`)
     so the header reflects the selection, not a hardcoded constant.
  2. **`app` - the dropdown.** `Widget` gains `selected`, `open`, and a
     `UniformListScrollHandle`. The header's right side becomes a `MUTED`,
     `cursor_pointer` button `"<label> v"` toggling `open`. When open, a
     `deferred` overlay renders an `occlude()` backdrop (click-away closes) plus a
     bounded `uniform_list` panel (fixed row height, `scroll_to_item(selected)`
     on open); each row selects + closes on click.
  3. **Re-point the source.** Store the refresh `Task<()>` on the `Widget` (not
     `detach()`); on selection, replace it (drop cancels the old 30-min loop) and
     fetch immediately; show the existing loading state during the switch.
  4. **Persistence.** New `app/src/persist.rs` (std-only, no new deps): read/write
     the selected id to `%APPDATA%\AirQlality\location.txt`; a write failure is
     ignored, a missing/corrupt file falls back to Cairo.
  5. **Test seam / evidence.** Core tests: `LOCATIONS[0]` is Cairo; `url()` carries
     the selected coords; `parse` carries the passed label. App: existing E2E
     verdict floors still pass plus an open-panel bite-proof; evidence
     `experiments/EXP07/EXP07-{closed,open}.png`, and `EXP07-restored.png` (a
     saved `tokyo-jp` id restored into the header on launch).
- May touch: `core/src/{lib,openmeteo,source}.rs`, new `core/src/location.rs`,
  `core/tests/**`, `app/src/{ui}.rs`, new `app/src/persist.rs`, `app/src/main.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP07/**`, `experiments-overview.md`,
  branch `EXP07` (from `93fc578`).
- Off-limits: `decision.rs` rule/thresholds, `UI.md`, `AGENTS.md`,
  `experiments/EXP01..06/**` (frozen evidence), the native popup-window path,
  declaring a final status.
- Machine-level blast radius: no new dependencies; creates one small user config
  file under `%APPDATA%`.

## Open questions (known unknowns)
- [x] Which locations - **curated static list of 9, Cairo first** (plan default).
- [x] Persist the selection - **yes, by stable id; Cairo on first/corrupt run**
      (user answer).
- [x] Row rendering - **`uniform_list` virtual panel** (fixed row height, bounded
      viewport, scroll handle, scroll-to-selected) (user answer).
- [x] List size / search - **short list, no search box** (user answer).
- [x] Interaction - **mouse-only** (click open, click row to select, click-away to
      close); keyboard/Esc deferred (plan default).
- [x] Mock mode - **selection changes the label only**; data stays the fixed
      Cairo-shaped mock so the E2E stays deterministic (plan default).
- [x] Base branch - **`EXP07` from `93fc578`** (plan default).
- [x] Scope - **one experiment** (dropdown + location plumbing + persistence)
      (plan default).

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** The fixed header label became a click-to-open dropdown, the
selection re-points the source and persists across restarts, and the mock E2E
stayed byte-identical — with no native popup and no new dependencies.

- **`core`.** New `location.rs` (`Location` + `LOCATIONS`, 9 cities, Cairo first,
  `DEFAULT_LOCATION`); `OpenMeteoSource::{for_location,from_location}` replace the
  Cairo-only constructor; `parse_openmeteo` takes the label so `Series.location`
  (and the header) track the selection.
- **`app`.** Header trigger toggles a `deferred` overlay: an `occlude` click-away
  backdrop + a bounded `uniform_list` panel (26 px rows, 7-row viewport,
  `scroll_to_item` on open). Rows select + close. Mouse-only.
- **Re-point without leaks.** The 30-minute refresh `Task` is stored on the widget;
  selecting replaces it, dropping (cancelling) the previous loop, then fetches the
  new location. Mock mode changes the label only.
- **Persistence.** `app/src/persist.rs` (std-only) reads/writes the stable id to
  `%APPDATA%\AirQlality\location.txt`; missing/corrupt → Cairo. Verified by
  restoring `tokyo-jp` into the header on launch.
- **Tests.** `core` 16/16; app persist 2/2; e2e 3/3 (verdict, forced error, and a
  new open-panel panel-pixel assertion). Verdict counts unchanged
  (`closed=2043 open=790 top_closed=1131`; `vent` 5666, down from 6293 because the
  fixed 36 px header trims the canvas); `panel=0` closed / ~62k open.
- **Evidence:** `EXP07-closed.png`, `EXP07-open.png` (Cairo highlighted, scrolls
  past 7 rows), `EXP07-restored.png` (persisted Tokyo restored). Seam
  `AIRQLALITY_OPEN_DROPDOWN=1`.
- **Not proven (out of scope):** the selection → **real HTTP** re-fetch is not
  black-box E2E'd (no click automation); mouse-only, no keyboard/Esc; no search;
  the list is a fixed 9.

**Recommended next:** a searchable, baked-in city list (EXP08).

## Learnings
- **MinGW must be on PATH for any app build.** `$env:PATH = "C:\Users\MW\AppData\Local\mingw64\bin;$env:PATH"; cargo build -p airqlality`.
  Without it, `psm`'s build script fails (`gcc.exe` not found) and linking fails
  (`ld: cannot find -lktmw32`). That mingw (gcc 16.2.0) carries
  `x86_64-w64-mingw32\lib\libktmw32.a`; rustup's bundled self-contained mingw does
  not. `cargo test`/`build` reuse the cached `psm` output, so only linking needs it.
  `app/tests/capture.ps1` already prepends this path for the launched app.
- **gpui `on_click` lives on `StatefulInteractiveElement`, not `Div`:** call
  `.id(...)` before `.on_click(...)` (the prelude brings the trait in scope).
- **`uniform_list` needs a definite height.** With the default
  `ListSizingBehavior::Auto` and no taffy children it lays out to height 0 and
  paints nothing; set `.h(px(..))` on the list itself (it implements `Styled`). A
  `max_h` on the parent is not enough.
- **Overlay that works:** `deferred(div().absolute()...)` as the last child of the
  root draws above the chart; `uniform_list` renders and scrolls fine inside it.
- **Deterministic dropdown screenshot:** `AIRQLALITY_OPEN_DROPDOWN=1` opens the
  panel at startup; with `AIRQLALITY_SOURCE=mock` + `AIRQLALITY_FREEZE_NOW=1` the
  E2E stays byte-identical.
- **Commands:** `cargo test -p airqlality-core` (16/16) and
  `cargo test -p airqlality` (persist 2/2 + e2e 3/3).
- **Regression numbers:** closed E2E verdicts unchanged
  (`closed=2043 open=790 top_closed=1131`); `vent` 5666 (was 6293 — the fixed
  36 px header shrinks the canvas a little); `panel=0` closed / `~62k` open.
- **Window captures race.** The black-box E2E and any manual `capture.ps1` run use
  the same centred window; run evidence captures serially, never beside `cargo test`.
