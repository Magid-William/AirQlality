# EXP08 - Searchable locations from a baked-in GeoNames city list

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Replace the fixed 9-location list with a searchable one: a committed GeoNames city
dataset baked into the binary, a hand-rolled search field in the dropdown panel,
virtualized results, and selection re-pointing `OpenMeteoSource` and persisting
(as in EXP07). Baked-in only — a miss shows "no matching city".

## Hypothesis
A committed GeoNames cities dataset plus a client-side filter rendered in gpui's
`uniform_list` gives a fast, offline, searchable location list; selection feeds the
existing source; and the mock E2E stays deterministic.

**Falsified if:** gpui-ce can't host a usable keyboard-focused text field without a
bundled widget, the baked dataset is too large to commit sanely, or search/select
can't be made deterministic for the E2E.

## Methodology & blast radius
- Method:
  1. **Data.** A one-off generator (`experiments/EXP08/make_cities.*`) downloads
     GeoNames `cities15000.zip` + `countryInfo.txt` and writes committed assets:
     `core/assets/cities.json` (`{id, name, ascii, country, lat, lon, tz, pop}`)
     and `core/assets/countries.json` (ISO alpha2 -> country name), plus an
     `ATTRIBUTION.md` (GeoNames CC BY 4.0).
  2. **`core`.** New `geo.rs`: `City`, `cities() -> &'static [City]`
     (`OnceLock` + `include_str!`), and a pure `search(query, limit) -> Vec<&City>`
     (case/diacritic-insensitive prefix match on `ascii`, ranked by population).
     `Location` becomes owned (`String`) and gains `From<&City>`; the static
     `LOCATIONS` of EXP07 is replaced by the baked set (Cairo found by geoname id).
     Persistence stores the geoname id (old `cairo-eg` -> Cairo default).
  3. **`app`.** The panel gains a focused search field (hand-rolled `FocusHandle` +
     `on_key_down`: chars/Backspace append/trim, Esc closes, Enter selects the top
     hit; rendered query + caret). Typing filters via `core::search`; the existing
     `uniform_list` renders results; empty query shows the current selection; no
     matches shows a "no matching city" row. Mouse/keyboard selection re-uses
     EXP07's re-point + persist path.
  4. **Test seam / evidence.** Core `search` unit tests (diacritics, ranking,
     empty/short query, no-match). App E2E: `AIRQLALITY_SEARCH` prefills a query so
     a deterministic search state can be screenshotted and bite-proofed
     (`experiments/EXP08/EXP08-search-*.png`); the closed/open panel E2E and the
     mock verdict counts stay unchanged.
- May touch: `core/src/{lib,openmeteo,source}.rs`, `core/src/location.rs` (rewritten),
  new `core/src/geo.rs`, new `core/assets/*`, `core/tests/**`, `app/src/{ui,persist}.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP08/**`, `experiments-overview.md`,
  branch `EXP08` (from `482a20c`).
- Off-limits: `decision.rs` rule/thresholds, `UI.md`, `AGENTS.md`,
  `experiments/EXP01..07/**` (frozen evidence), the native popup path, declaring a
  final status.
- Machine-level blast radius: commits a ~2-4 MB dataset; no new crate dependencies.

## Open questions (known unknowns)
- [x] Dataset scope - **GeoNames cities15000 (~25k cities)** as "the supported
      cities"; regenerable to cities500 if broader coverage is wanted (plan default,
      assumption).
- [x] Live Open-Meteo geocoding fallback - **no**, baked-in only; a miss shows "no
      matching city" (user answer).
- [x] Search field - **hand-rolled minimal gpui field**, Latin/diacritic only, no IME
      this experiment (plan default).
- [x] Attribution - **add GeoNames (CC BY 4.0)** beside Open-Meteo (plan default).
- [x] Old curated 9 - **replaced**; Cairo stays the first-run default (plan default).
- [x] Base branch - **`EXP08` from `482a20c`** (plan default).

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** A committed GeoNames city dataset plus a client-side filter
in `uniform_list` gives an offline, searchable location list; selection feeds the
existing `OpenMeteoSource`; the mock E2E stayed deterministic — with no new crate
dependencies.

- **Data.** `experiments/EXP08/make_cities.ps1` derives `core/assets/cities.json`
  (34,152 GeoNames `cities15000` cities, 4.35 MB) + `countries.json` (ISO -> name),
  baked via `include_str!` and parsed once (`OnceLock`). CC BY 4.0, attributed in
  `core/assets/ATTRIBUTION.md` and the panel.
- **`core`.** `geo.rs`: `City`, pure `search(query, limit)` (case/diacritic-folded
  prefix match, population-ranked), `city_by_id`, `country_name`. `Location` is now
  owned (`From<&City>`); the fixed 9-item `LOCATIONS` is gone; Cairo stays the
  default via its GeoNames id.
- **`app`.** The panel gained a hand-rolled search field (`FocusHandle` +
  `track_focus` + `on_key_down`, text via `key_char`; Esc closes, Enter selects the
  top hit). Results render in the existing `uniform_list`; a short query shows
  "Type at least 2 letters", a miss shows "No matching city". Selection re-points
  the source (cancels the old refresh) and persists the GeoNames id.
- **Tests.** `core` 21/21; app persist 2/2; e2e 5/5 — including a new test that
  posts **real keystrokes** to the window and asserts the results list paints
  (`panel=92182`), plus a no-match state test.
- **Evidence:** `EXP08-search-{paris,nomatch,empty,typed}.png`. Closed-screen
  verdicts unchanged (`closed=2043 open=790`); panel `0` closed / `~92k`
  open-with-results / `~29k` hint.
- **Not proven (out of scope):** Latin/diacritic search only (no IME/non-Latin);
  the selection -> **real HTTP** re-fetch is not black-box E2E'd; the dataset is
  `cities15000`, so smaller places are absent (regenerable to `cities500`); the
  binary/repo grows by ~4.5 MB.

**Recommended next:** cache the last good response to disk (survive restarts
offline), then the always-on-top + tray frontier EXP01 flagged.

## Learnings
- **Data generation:** `pwsh -File experiments/EXP08/make_cities.ps1` downloads
  GeoNames `cities15000` + `countryInfo` and writes `core/assets/cities.json`
  (34,152 cities, 4.35 MB) + `countries.json`. `include_str!` bakes them in; parse
  once via `OnceLock` on first use (~0.5 s in tests, at startup in the app).
- **gpui text field (no widget):** `cx.focus_handle()` + `div().track_focus(&h)`
  + `window.focus(&h, cx)` on open; handle keys with `on_key_down`, reading
  `ev.keystroke.key` for named keys and `ev.keystroke.key_char` for text. Set
  `focusable` via `track_focus`. No IME/non-Latin this experiment.
- **`on_key_down`/`on_click` on `Div` need the `InteractiveElement` /
  `StatefulInteractiveElement` traits in scope** (the prelude brings them).
- **Focus the field on the first open frame** when the panel is opened from an env
  seam (there is no click to call `toggle_open`); do it in `render` where `window`
  is available.
- **Driving real input in tests:** `SendKeys` is unreliable (Windows will not hand
  focus to the child process here). Post `WM_KEYDOWN`/`WM_CHAR`/`WM_KEYUP` to the
  window handle instead (`capture.ps1 -Type "tokyo"`); the app receives them and
  the search field updates.
- **Commands:** `cargo test -p airqlality-core` (21/21) and
  `cargo test -p airqlality` (persist 2/2 + e2e 5/5).
- **Regression numbers:** closed E2E verdicts unchanged
  (`closed=2043 open=790 top_closed=1131`); panel `0` closed, `~92k` open with
  results, `~29k` for the short hint/no-match panel (the numbers the E2E keys off).
- **`uniform_list` still needs a definite height** (`.h(px(..))` on the list).
