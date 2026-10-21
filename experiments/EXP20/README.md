# EXP20 - Daily shape: say when the good hours are, not the longest run

**Status:** `success` (user-declared, 2026-10-15).

## Summary
`best_window` collapses 48 h of hourly verdicts into one contiguous span, so the
widget can print `best window 03:00 - 21:00 (19 h)` and say nothing about the
coming night being three times worse. Add a pure, truthful read of the day's
shape: a 24 h band census plus the next clean hour, on the banner.

## Hypothesis
For Cairo the day has a stable shape - a post-inversion morning peak and a
mid-afternoon trough (CAMS hour-of-day means over two years: 06:00-08:00 =
27.4-28.0 vs 15:00-17:00 = 17.2-18.1 µg/m³, corroborated by the black-cloud
literature and by aqi.in's own 24 h chart) - therefore

- (a) the longest contiguous `Open` run is a poor and sometimes actively
  misleading summary of a day, and
- (b) a 24 h band census plus "the next clean hour" is a strictly more
  actionable line at equal screen cost.

**Falsified if:** on the frozen mock the census adds no information over
`best_window` (both would have to say "5 clean hours at 02:00"), or a live Cairo
forecast exists where the longest run and the census agree - the extra line would
then be noise and should be cut, not kept.

## Methodology & blast radius
- **Pure model (`core`).** New `core/src/dayshape.rs`:
  `band_census(&[Verdict], now_index) -> DayShape { hours_covered, clean,
  mediocre, closed, first_clean: Option<usize> }`, horizon `HORIZON_HOURS = 24`
  from "now" inclusive, clamped at the end of the series (`hours_covered` says how
  many hours it really saw). `decision::longest_run` is clamped to the same
  horizon, so the banner's existing "in the next 24 h" claim becomes true instead
  of searching the whole 48 h series.
- **Wiring.** `Decision` gains `pub shape: DayShape`, filled in
  `evaluate_hourly` - the single place `evaluate` and `fuse` already funnel
  through, so the multi-source path gets the census for free and no call site
  changes.
- **App.** One added `text_xs` MUTED line under the banner subline:
  `next 24 h: 12 clean · 11 mediocre · 1 closed · clean from 18:00`. The chart's
  text alternative gains the same census. Nothing is painted, so `ChartGeom`,
  hover, the noise overlay and the spread band are untouched.
- **Tests.** `core` units: the mock's exact census (5/17/2, first clean at 35),
  clamp at the end of a short series, all-clean, no-clean-in-horizon,
  `hours_until_first_clean`, a run that starts beyond the horizon not being
  reported as `best_window`, and a census that says what the run leaves out.
  `app` units: the banner line and the chart's sentence, pinned word for word on
  the mock, plus the two cases that must *not* announce an upcoming clean hour
  (clean right now, and no clean hour at all). App e2e: the frozen-mock colour
  counts before/after.
- **Evidence.** A keyless `#[ignore]`d report test
  (`live_cairo_history_report`) that prints the CAMS month means and hour-of-day
  means - the numbers the hypothesis rests on - plus `EXP20-*.png` captures and
  the aqi.in prior-art study.
- **May touch:** `core/src/{dayshape.rs(new),lib.rs,decision.rs}`,
  `core/tests/live_sources.rs`, `app/src/ui.rs`,
  `app/tests/e2e_widget.rs` (not needed in the end: the mock counts did not
  move), `experiments/EXP20/**`, `experiments-overview.md`, branch `EXP20` (from
  `a9524e2`).
- **Off-limits:** threshold semantics and values (`open_max` / `vent_max`),
  `fuse.rs` median / tie-break / staleness, `noise.rs`, tray / icon / startup /
  notification code, `experiments/EXP01..19/**` (frozen evidence), `UI.md`,
  `AGENTS.md`, declaring a final status.
- Machine-level blast radius: none - no new crate, no new dependency, no new
  runtime network call.

## Open questions (known unknowns)
- [ ] **Horizon** - 24 h from `now` inclusive, clamped at the series end, with
      `hours_covered` exposed. *(Assumption adopted.)*
- [ ] **Additive or replacing** - `best_window` keeps its meaning and its line;
      the census is a second line. *(Assumption adopted: EXP19's fusion and every
      existing test stay untouched.)*
- [ ] **Frozen mock baseline** - the census renders on the mock too, so the e2e
      counts may move. Default: re-baseline and record old -> new; the seam
      discipline (`AIRQLALITY_NOISE`, `=mock3`) is for overlays that must not
      perturb the view, not for the app's core sentence. *(Assumption adopted;
      `AIRQLALITY_SHAPE=on|off` remains the fallback if the counts must stay
      frozen.)*
- [ ] **Width at 560 px** - `next 24 h: 12 clean · 11 mediocre · 1 closed · clean
      from 18:00` is ~62 chars at `text_xs`; if it collides, drop the tail
      first, then the `next 24 h:` prefix, never the three counts. *(Assumption
      adopted; it fits with room to spare, see `EXP20-real-cairo.png`.)*
- [ ] **Colour of the census** - MUTED only. Painting the counts in the verdict
      colours would put green (`OPEN`) pixels in the banner, which is in the top
      35 % of rows where the e2e asserts `top_open == 0` for a closed verdict.
      *(Assumption adopted.)*
- [ ] **Day/night shading on the canvas** - later, in its own experiment: it
      moves pixels and needs its own seam and evidence. *(Assumption adopted.)*
- [ ] **Words** - `clean / mediocre / closed`, mirroring the three verdict words;
      never `Good / Moderate / Poor`. *(Assumption adopted - EXP01: the verdict
      vocabulary is the differentiator.)*

## Live probe (2026-10-15, Cairo)

The evidence the hypothesis rests on, reproducible with one keyless request:

```text
cargo test -p airqlality-core --test live_sources live_cairo_history_report -- --ignored --nocapture
```

```text
Cairo archive: 17568 hours, 2024-10-14T00:00 .. 2026-10-15T23:00 (source: CAMS via Open-Meteo)

hour  mean PM2.5 (µg/m³)        hour  mean PM2.5
  00       25.7                  12       20.5
  04       26.5                  16       17.1
  06       27.1                  17       17.0   <- best
  07       27.6   <- worst       20       23.4

worst hour 07 = 27.6 µg/m³, best hour 17 = 17.0 µg/m³

window               mean PM2.5   mean AQI (US)   hours
  2024 (partial)       23.0           77          1896
  2025 full year       23.6           78          8760
  2025 Jan-Oct 14      23.6           78          6888
  2026 Jan-Oct 14      23.0           77          6888
  Oct 1-13 2025        17.1           66           312
  Oct 1-13 2026        17.9           67           312
```

What it establishes:

- **The day has a shape and it is stable**: worst hour 07:00 (27.6), best hour
  17:00 (17.0), a 10.6 µg/m³ swing that is larger than the whole 2025-2026 year
  over year difference. A daily average, or one long "best window", is the wrong
  resolution for this decision.
- **The aqi.in year over year claim does not reproduce** (see
  `prior-art-aqi-in.md`): like-for-like, 2025 Jan-Oct 14 = 23.6 vs 2026 = 23.0.
- **The month shape**: January worst (36.4 / 36.5), June to October best
  (16.4-20.4). October is one of the *cleanest* months on average in Cairo, so
  the black cloud is episodic, not a monthly level - another reason to answer in
  hours, not months.
- **CAMS is a 45 km model archive**, not a station: use it for shape and trend,
  never for level (IQAir's station network put Cairo's annual PM2.5 at 40.8 for
  2025; this archive says 23.6).

## What the app shows

Mock (frozen, `AIRQLALITY_SOURCE=mock` + `AIRQLALITY_FREEZE_NOW=1`):

```text
before   KEEP CLOSED | PM2.5 26 µg/m³ now · best window 02:00 - 06:00 (5 h)
after    KEEP CLOSED | PM2.5 26 µg/m³ now · best window 02:00 - 06:00 (5 h)
                       next 24 h: 5 clean · 17 mediocre · 2 closed · clean from 02:00
```

Real Cairo (2026-10-15, live Open-Meteo, nothing faked):

```text
VENT | PM2.5 19 µg/m³ now · best window 10:00 - 15:00 (6 h)
     | next 24 h: 12 clean · 11 mediocre · 1 closed · clean from 18:00
```

Three things the census adds there that the run alone never said: the air is
**mediocre right now** (not a hidden "closed"), it turns **clean from 18:00**, and
**12 of the next 24 hours are openable** even though the best single stretch is
tomorrow morning.

Evidence: `EXP20-before.png`, `EXP20-mock.png`, `EXP20-real-cairo.png`.

The wording itself is pinned by `app` unit tests (the E2E can only count pixels,
and the counts are floors, so nothing else would catch a bad string):

```text
day_shape_line  == "next 24 h: 5 clean · 17 mediocre · 2 closed · clean from 02:00"
sentence        == "In the next 24 hours, 5 clean, 17 mediocre and 2 to keep the
                    window closed. The first clean hour is at 02:00."
all clean       == "next 24 h: 24 clean · 0 mediocre · 0 closed"   (no "clean from")
all closed      == "next 24 h: 0 clean · 0 mediocre · 24 closed"
```

Suite, whole workspace: **app 37/37** (+4), **e2e 23/23** (unchanged),
**core 60/60** (+8), 4 live probes ignored.

Pixel cost, measured on the frozen mock (`widget_e2e_renders_expected_verdict`):

| | closed | vent | open | bg | top_closed | top_open | panel |
|---|---|---|---|---|---|---|---|
| before | 2043 | 5670 | 790 | 272921 | 1131 | 0 | 0 |
| after | 2043 | 5668 | 790 | 270545 | 1131 | 0 | 0 |

The line costs 2 376 background pixels (the canvas is `flex_1`, so it shrinks by
the new row) and **moves no verdict colour**: `open=790`, `closed=2043` and
`top_open=0` are untouched, the only movement is 2 VENT pixels. So a text-only
change needed no re-baseline and no seam - the E2E's base counts still hold.

## Conclusion (findings)

**Concluded `success` by the user on 2026-10-15.**

**Hypothesis upheld.** Both halves held. The longest contiguous `Open` run really
is a poor summary of a Cairo day (its own unit test now pins the case: a 10-hour
run reported alone, hiding the 14 dark hours in front of it), and the 24 h band
census plus the first clean hour is a stricter, more useful line at the same
screen cost - on the live frame it says *"12 clean · 11 mediocre · 1 closed · clean
from 18:00"*, which the run never said: the air is mediocre *right now*, it turns
clean at 18:00, and 12 of the next 24 hours are openable even though the best
single stretch is tomorrow morning.

- **Model.** `core::dayshape::{band_census, DayShape, HORIZON_HOURS}` (24 h from
  "now" inclusive, clamped at the end of the series, `hours_covered` exposed);
  `Decision` carries it, filled in `evaluate_hourly`, so EXP19's fusion got it for
  free. `longest_run` is clamped to the same horizon - which is what makes the
  banner's pre-existing *"no clean window in the next 24 h"* text true, since it
  used to walk the whole 48 h series.
- **App.** One MUTED `text_xs` banner line plus the same census in the chart's
  text alternative; wording pinned word for word by four `app` unit tests,
  including the two cases that must *not* announce an upcoming clean hour (clean
  right now, and no clean hour at all).
- **Evidence.** The keyless CAMS probe - worst hour 07:00 = 27.6 vs best hour
  17:00 = 17.0 µg/m³, a 10.6 swing larger than the whole year over year
  difference; like-for-like 2025 vs 2026 Jan-Oct 14 = 23.6 vs 23.0 µg/m³, against
  aqi.in's claimed -35.2 %. Mock before/after, the live Cairo frame, and the
  aqi.in study in `prior-art-aqi-in.md`.
- **The unplanned finding.** The census did not need a new seam: because the
  canvas is `flex_1`, a text-only change cost 2 376 background pixels and moved
  **2** verdict pixels (`open=790` and `closed=2043` identical), so the frozen
  E2E counts still hold and no `AIRQLALITY_SHAPE` switch was needed.
- **Not proven, and out of scope.** The "stable shape" is a *climatology* from a
  45 km model archive, not Cairo's measured diurnal cycle; the census counts
  hourly verdicts, so it cannot know that 05:00 and 07:00 differ in kind; and the
  widget still has nothing to compare today against - the honest
  history/baseline layer (last 30 days, or same-month-last-year, always labelled
  with its window and day count) is argued for and unbuilt.

**Recommended next:** the honest history layer (the aqi.in study's real lesson),
real solar day/night boundaries shared with EXP21, and a notification body that
carries "clean from HH:MM".

## Learnings
- **Pin banner wording in an `app` unit test, not via the a11y JSON dump.** E2E
  pixel counts are floors, so nothing there catches a bad string, and a manual
  `capture.ps1 -A11y` produced no `AIRQLALITY_A11Y` output (the dump needs
  AccessKit actually activated; the E2E's own sequence does). `day_shape_line` /
  `day_shape_sentence` are free functions on `&Series` + `&Decision`, so test them
  directly.
- **A text-only UI change needs no seam**: a `flex_1` canvas absorbs the new row
  (only `bg` drops, 272 921 -> 270 545), so the frozen E2E counts survive. Seam
  discipline is for *painted* overlays (EXP16 noise, EXP19 band, EXP21 wave).
- **A banner line must stay MUTED**: the banner is in the top 35 % of rows, where
  the E2E asserts `top_open == 0` for a closed verdict, so colouring the "5 clean"
  count green would break that guarantee for no information the words lack.
- **`ureq` needs the TLS backend handed to it, including in tests**: an
  integration test in `core/tests` cannot use the private `core::http`, so build
  the agent itself (`native_tls::TlsConnector` + `AgentBuilder::tls_connector`),
  or the probe dies with "Unknown Scheme: ... no TLS backend is configured".
- **One keyless `start_date`/`end_date` request to the air-quality endpoint
  returns two years of Cairo hours** (17 568) in ~1 s, with `chrono::Local::now()`
  available from the default `clock` feature. Use it whenever a claim needs a past
  window - and label a delta with its window, its day count and its dataset.
