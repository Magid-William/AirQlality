# EXP21 - Day/night wave on the timeline

**Status:** `success` (user-declared, 2026-10-15).

## Summary
EXP20 gave the banner the day's *numbers* ("next 24 h: 12 clean · 11 mediocre · 1
closed · clean from 18:00"). This gives the same fact to the eye: shade the
night hours behind the curve, with the boundary drawn as a smooth wave that
peaks at local midnight and lies flat at local noon.

## Hypothesis
A night region behind the curve, shaped by a documented cosine of the local hour,
makes the day's shape legible without reading any text - the morning peak and the
bad evening sit inside the shaded band, the clean afternoon sits in the light -
and it does so without touching a single number: the verdict, the census, the
curve and the threshold lines are all unchanged, and the frozen mock view only
moves when the layer is switched on.

**Falsified if:** the shaded region makes any verdict-colour count move, or the
wave cannot be told apart from the min-max spread band (EXP19) and the noise
overlay (EXP16) in the same frame - in that case it is decoration and should be
cut, not kept.

## Methodology & blast radius
- **Pure model (`core`).** `core/src/dayshape.rs` gains
  `night_amount(hour: f32) -> f32` - `0.0` at local noon, `1.0` at local
  midnight, `0.5` at 06:00 and 18:00, a cosine so the wave has no corners - plus
  `night_curve(&Series) -> Vec<f32>`, one weight per point from that point's
  local hour, and the documented `DAY_START_HOUR = 6` / `DAY_END_HOUR = 18`
  window. No `chrono` logic in the app (EXP16's learning).
- **App.** `theme::NIGHT` (a dark navy tint), and `draw_timeline` paints the
  region first, as the lowest layer: a filled path from the wave boundary down to
  the axis, so the grid, the spread band, the noise curve and the PM2.5 curve all
  stay on top. The footer gains a `night` key when the layer is on, and the
  chart's text alternative says what is shaded.
- **Test seam.** `AIRQLALITY_NIGHT=on|off`, default **on** for the real app and
  **off** under `AIRQLALITY_SOURCE=mock`, exactly like `AIRQLALITY_NOISE`
  (EXP16), so every pre-EXP21 screenshot stays byte-identical.
- **Tests.** `core` units: the wave's anchor points (noon 0, midnight 1, dawn and
  dusk 0.5), that it is a smooth rise and fall with no discontinuity at midnight,
  and that `night_curve` follows each point's own local hour across a day
  boundary. `app` e2e: the default mock run paints **zero** night pixels; the
  seam-on mock run paints a shaded region with the same verdict-colour counts.
- **Evidence.** `EXP21-*.png` (mock with the wave on, real Cairo, and the
  pre-EXP21 frame for comparison) plus the e2e colour counts.
- **May touch:** `core/src/dayshape.rs`, `app/src/{theme.rs,ui.rs}`,
  `app/tests/e2e_widget.rs`, `experiments/EXP21/**`, `experiments-overview.md`,
  branch `EXP21` (from `a8b1618`).
- **Off-limits:** threshold values and semantics, `fuse.rs`, `noise.rs`, the
  verdict vocabulary, tray / icon / startup / notification code, the census and
  `best_window` semantics from EXP20, `experiments/EXP01..20/**`, declaring a
  final status.
- Machine-level blast radius: none - no new crate, no new dependency, no new
  network call.

## Open questions (known unknowns)
- [ ] **What "day" means** - a **fixed 06:00-18:00 local window**, documented as
      an approximation: Cairo's real sunrise/sunset swing it by about an hour and
      a half across the year, and the widget has no solar geometry. *(Assumption
      adopted; real twilight is a later refinement.)*
- [ ] **Wave shape** - a cosine weight, shaded from the axis up, peak at midnight,
      flat at noon. *(Assumption adopted.)*
- [ ] **Amplitude** - 45 % of the plot height: clearly visible, never reaching the
      PM2.5 curve's usual band. *(Assumption adopted.)*
- [ ] **Colour** - **darker than `BG`** (`NIGHT = 0x05070d`): a night band that
      is *lighter* than the canvas reads as highlighted - daylight - which is
      exactly backwards. First painted at `0x172038` and changed on sight. The
      footer key therefore carries a 1px `AXIS` outline, because a
      darker-than-background swatch on a dark footer otherwise reads as nothing.
      *(Assumption adopted, then corrected by looking at it.)*
- [ ] **Legend key** - a `night` swatch **only when the layer is on**, and only if
      it fits: the footer is already full at 560 px with `noise est.` present
      (EXP16). *Measured:* with the key, the row overflowed and clipped the axis
      key ("dashed forecas...") by ~22 px, so the axis key drops the two
      line-style words **when - and only when - the night key is present**:
      `PM2.5 µg/m³ · past · forecast`. The default mock footer is therefore
      unchanged, and a real user always sees the short form with the `night` key.
      *(Assumption adopted, then measured.)*
- [ ] **Any number changes?** - **no**. Verdict, census, curve and thresholds are
      untouched; this layer is paint only. *(Assumption adopted.)*
- [ ] **Order of layers** - night is the lowest layer, under the spread band, the
      noise curve and the PM2.5 curve. *(Assumption adopted.)*

## What the app shows

Mock, night layer off vs on (both with the noise overlay on, so the busy case is
what is compared):

```text
off   KEEP CLOSED | PM2.5 26 µg/m³ now · best window 02:00 - 06:00 (5 h)
                    next 24 h: 5 clean · 17 mediocre · 2 closed · clean from 02:00
on    ...same two lines, plus two shaded night humps behind the curve and a
      `night` key in the footer
```

Real Cairo (2026-10-15, live Open-Meteo, wave and noise overlay on by default):

```text
VENT | PM2.5 16 µg/m³ now · best window 10:00 - 16:00 (7 h)
     | next 24 h: 13 clean · 10 mediocre · 1 closed · clean from 18:00
```

`EXP21-mock-no-wave.png`, `EXP21-mock-night.png`, `EXP21-real-cairo.png`.

In the real frame the shape now reads without any text: the PM2.5 peaks sit
*inside* the shaded humps, the clean stretch sits in the light between them, and
the dotted noise curve runs through both - three layers that stay apart, which is
what the falsification clause asked.

The chart's text alternative gains: *"Night is shaded, from 18:00 to 06:00."*

## Evidence (pixels)

| frame | night | closed | open | note |
|---|---|---|---|---|
| default mock (`AIRQLALITY_NIGHT` unset, off under the mock) | **0** | 2043 | 790 | the frozen pre-EXP21 baseline |
| mock with `AIRQLALITY_NIGHT=off` | **0** | - | - | the explicit seam value behaves the same |
| mock with `AIRQLALITY_NIGHT=on` | **27195** | 2043 | 790 | shading painted, verdict colours untouched |

The last row is the falsification check that matters: the night fill is the
lowest layer, so it moves no verdict pixel (`closed=2043`, `open=790` are the
frozen values) - it is paint only. The first two rows are what keeps every
pre-EXP21 screenshot byte-identical.

Suite, whole workspace: **core 64/64** (+4), **app 37/37** (unchanged),
**e2e 24/24** (+1), 5 live probes ignored. One run of the suite failed
`widget_e2e_tab_shows_focus_ring` (`focus_in_band=0`) and passed in isolation with
the documented 1280 - the pre-existing E2E flake under load, not this layer (the
full suite was green on a re-run).

## Conclusion (findings)

**Concluded `success` by the user on 2026-10-15.**

**Hypothesis upheld.** The wave ships, and both falsification clauses were checked
in pixels rather than prose. The night fill is the lowest layer, so it moves no
verdict pixel (`closed=2043`, `open=790` are the frozen values); in the real frame
the PM2.5 peaks sit *inside* the shaded humps while the dotted noise curve runs
through both, so the night band, the spread/noise layers and the curve stay apart.

- **Model.** `night_amount` (a cosine of the local hour: `0.0` at noon, `1.0` at
  midnight, `0.5` at 06:00 and 18:00), `night_curve` (one weight per point, from
  that point's own local hour, so it crosses a day boundary), `is_daytime`, and
  the documented `DAY_START_HOUR` / `DAY_END_HOUR` window. Pure, no `chrono`
  logic in the app.
- **App.** `theme::NIGHT = 0x05070d` - **darker than the canvas** - painted as one
  filled polygon first, with a conditional, outlined `night` footer key and
  *"Night is shaded, from 18:00 to 06:00."* in the chart's text alternative.
- **Evidence.** `EXP21-{mock-no-wave,mock-night,real-cairo}.png`; night pixels
  `0` by default, `27195` with the seam on, `0` with the seam explicitly off;
  verdict colours untouched; suite core 64/64, app 37/37, e2e 24/24.
- **Corrected on sight.** The first fill (`0x172038`) is *lighter* than `BG`, and
  a lighter band reads as daylight - the exact opposite of the meaning - so it was
  inverted to below the canvas. Two knock-ons: the darker swatch needed a 1px
  outline in the footer, and `legend_item` had to be split so `bg`/`border` land
  on the 8x8 swatch while `flex`/`gap` stay on the row.
- **Not proven, and out of scope.** This is an *aid*, not an input: no number,
  verdict, threshold or geometry depends on it. The day window is fixed
  wall-clock 06:00-18:00, while Cairo's real dawn and dusk swing it by ~1.5 h
  across the year; the dawn/dusk transition is drawn as a smooth cosine although
  the real one is fast; there is no seasonal or weekday variation; and the layer
  can only be switched off through the `AIRQLALITY_NIGHT` seam, not from the UI.

**Recommended next:** real sunrise/sunset per location and date (pure solar-position
math, no key) to replace the fixed window; a tray toggle for the layer, with a
checkmark like "Run at startup"; and the honest history layer that the aqi.in study
argued for but that nobody has built yet.

## Learnings
- `cargo test -p airqlality-core` / `-p airqlality --bin airqlality` while
  iterating (no window); the windowed E2E suite **once, at the end** - each E2E
  test spawns the real widget, so a full run flashes windows for minutes
  (AGENTS.md). One run of the suite failed `widget_e2e_tab_shows_focus_ring` with
  `focus_in_band=0` and passed in isolation with the documented 1280: the
  pre-existing E2E flake under load, not this layer.
- **A "night" region must be darker than the canvas, not lighter.** On a dark
  theme a lighter band reads as *highlighted* (daytime): `0x172038` had to become
  `0x05070d`. Knock-on: a darker-than-background legend swatch is invisible on a
  dark footer, so the key borrows a 1px `AXIS` border - inside the 8x8 box, so the
  row's width does not change.
- **The footer is a one-line budget at 560 px, and it was already spent.** A 5th
  legend item clipped the axis key by ~22 px; freeing the space from the *axis
  key* (dropping `N `, `solid`, `dashed`, which the chart shows anyway) costs no
  information, while dropping `est.` from `noise est.` would cost honesty about a
  modelled number. Change it **conditionally on the key being present**, so the
  default mock frame keeps the exact footer it had before.
- **An area behind the curve is one filled `PathBuilder::fill()` polygon**, not a
  column of quads: `move_to` the axis corner, `line_to` each weighted point, close
  along the axis. Painting it first is what makes it the lowest layer, and the
  antialiased edge means the test asserts a floor (27 195 px on, 0 off) and that
  the verdict colours did not move - one test proving both "it is painted" and
  "it is paint only".
- **`legend_item`'s swatch is the inner 8x8 div**: `bg`/`border` belong there and
  `flex`/`gap` on the row, or the label gets boxed instead.
