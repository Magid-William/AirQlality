# EXP16 - Infer a noise curve from open geodata (overlay on the timeline)

**Status:** `success` (user-declared, 2026-10-05).

## Summary
There is no free, live, measured noise feed for Cairo. Instead, infer an hourly
noise estimate from open geodata (road density + population for the level, a
documented diurnal traffic profile for the shape), calibrate it against
published Cairo literature, and draw it as a subordinate second curve on the
existing PM2.5 timeline.

## Hypothesis
A defensible quiet-hours curve can be produced with **no noise measurement** by
combining (a) a spatial base level from open geodata and (b) a documented
diurnal traffic profile, such that the modelled Cairo shape reproduces the
literature's diurnal pattern (night minimum ~03:00-05:00, morning/evening rush
peaks) and lands within a stated band of published Cairo LAeq (~60-65 dB(A)
night, ~75-85 dB(A) day). It renders as a second, clearly-labelled estimate
curve without disturbing the frozen byte-identical AQ view, and locations
without geodata degrade gracefully.

**Falsified if:** the model cannot reproduce the Cairo literature shape within
the stated band; the output is not deterministic/offline-testable; or the
overlay perturbs the existing baseline (`closed=2043 vent=5600 open=790
panel=0`).

## Methodology & blast radius
- **Data (open, no key).**
  - **GRIP4 road-density rasters** (Global Roads Inventory Project, ODbL) -
    road m/km^2, 5 arcmin, split by road type; the global total raster is
    ~3.6 MB and is sampled once at build time. City-level spatial signal.
  - **GeoNames population** (CC BY 4.0) - already baked in `core/assets/cities.json`
    (`City.pop`); a second, already-present spatial signal.
  - **Published Cairo measurements** (2001-2026) - not consumed at runtime; used
    to calibrate/validate the level and the shape (they live in the README).
- **Model (pure, `core`).**
  - `TrafficProfile` - the documented diurnal shape in dB relative to the daily
    peak (0 dB at rush, floor ~ -9 dB at 03:00-05:00), constants documented with
    the citation that anchors them.
  - `estimate_hourly(base, profile) -> [f32; 24]` - `L(h) = base + profile[h]`.
  - `base` from a small mapping of `(population, road_density)` to a dB(A)
    level, calibrated so Cairo matches the literature band.
  - New `core/src/noise.rs`; `model`/`decision` stay pure and untouched.
- **Bake.** A generator script (like EXP08's `make_cities.ps1`) downloads GRIP4,
  samples road density at each baked city, and writes a compact
  `core/assets/noise_density.json` (city id -> density). Committed.
- **App (`app`).** A dotted violet curve (`theme::NOISE`) mapped
  absolute-hour -> hour-of-day -> `L(h)` on a **fixed 45-90 dB(A) scale**,
  drawn under the PM2.5 curve; the hover tooltip gains `Noise ~NN dB(A) est.`;
  a legend entry ("noise est.") shown only when the overlay is on; GRIP4 (ODbL)
  attribution in the location panel. Show-only: **no change to the AQ verdict
  or best window**; a location with no baked density simply shows no curve.
- **Test seam.** `AIRQLALITY_NOISE=on|off`; default **on** for the real app and
  **off** under `AIRQLALITY_SOURCE=mock`, so the EXP04/05/13 byte-identical
  baseline is preserved while the new e2e opts in.
- **Evidence.** core **26/26** (4 new: profile shape, base-level monotonicity +
  clamps, Cairo band, midnight hour-mapping); app e2e **20/20** (new
  `widget_e2e_shows_noise_overlay`: `on=321` pixels, `off=0`); screenshots
  `experiments/EXP16/EXP16-noise.png` (mock, curve + legend), `EXP16-noise-hover.png`
  (tooltip `Noise ~73 dB(A) est.`), `EXP16-no-noise.png` (default, unchanged),
  `EXP16-real-cairo.png` (real Open-Meteo + inferred noise).
- **May touch:** `core/src/{noise.rs(new),lib.rs}`, `core/tests/**`,
  `core/assets/{noise_density.json(new),ATTRIBUTION.md}` + the generator,
  `app/src/{ui.rs,theme.rs}`, `app/tests/e2e_widget.rs`,
  `experiments/EXP16/**`, `experiments-overview.md`, branch `EXP16` (from
  `2787774`).
- **Off-limits:** `UI.md`, `AGENTS.md`, the `decision.rs` thresholds/logic,
  `experiments/EXP01..15/**` (frozen evidence), declaring a final status.
- Machine-level blast radius: one ~3.6 MB ODbL download at build time; no new
  crate; no new runtime network call.

## Open questions (known unknowns)
- [x] **Spatial fidelity** - **city-level** (GRIP road density + GeoNames
  population). Street-level (bake OSM major-road distance) is a later
  refinement. *(Assumption adopted.)*
- [x] **Diurnal shape** - **documented profile anchored to Cairo literature**,
  offline, no key. A free-keyed traffic API (TomTom) is a named follow-up.
  *(Assumption adopted.)*
- [x] **Absolute level** - `A + B*log10(density+1) + C*log10(population+1)`
  (A=28, B=8, C=2), clamped to 45-88 dB(A), anchored so Cairo fits the
  literature band. *(Assumption adopted.)*
- [x] **Does it change the verdict?** - **no**, informational overlay only.
  *(Assumption adopted.)*
- [x] **Weekday/weekend** - **single profile** in v1. *(Assumption adopted.)*
- [x] **Overlay style** - a **dotted violet curve on a fixed 45-90 dB(A) scale**,
  subordinate to PM2.5; no separate axis line (the chart has none), so the
  legend ("noise est.") and the tooltip carry the numbers. *(Assumption
  adopted; a right-hand axis was dropped to match the existing chart.)*
- [x] **GRIP parsing** - **solved**: the GRIP4 download is **ESRI ASCII grids**
  (`.asc`, plain text), not GeoTIFF. No GDAL, no Pillow - `numpy.fromstring`
  parses it. Fallback not needed.

## Validation (interim)
Every constant is documented in `core/src/noise.rs`. Modelled peak/night levels
(dB(A)) versus the Greater-Cairo literature (roadside LAeq ~63-85 by day,
dropping overnight; the metro study's 24 h trace falls from ~75 to ~60):

| City | GRIP density (m/km^2) | Peak | Night floor |
|---|---|---|---|
| Cairo | 12,677 | 74.8 | 62.8 |
| Alexandria | 23,610 | 76.4 | 64.4 |
| Tokyo | 15,766 | 75.6 | 63.6 |
| London | 10,317 | 74.0 | 62.0 |
| Paris | 12,484 | 73.4 | 61.4 |

The shape (a night floor near 04:00, morning and evening peaks) and the level
both land inside the Cairo literature band. This is an **estimate**, not a
measurement, and the UI labels it as such.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-05.**

**Hypothesis upheld.** A city-level noise curve was inferred with **no
measurement**, from open geodata alone, and overlaid on the timeline. It
reproduces the Cairo diurnal shape and level, renders as a subordinate
"estimate" curve, and leaves the frozen AQ view byte-identical.

- **Model.** `core/src/noise.rs`: `L(h) = 28 + 8*log10(density+1) +
  2*log10(pop+1) + profile[h]`, clamped to 45-88 dB(A). Cairo peaks at 74.8 and
  floors at 62.8 dB(A) - inside the published Greater-Cairo band (roadside LAeq
  ~63-85 by day; the metro 24 h trace ~75 falling to ~60). Every constant is
  documented.
- **Data.** GRIP4 road density (ODbL; **ESRI ASCII**, so no GDAL) sampled at
  each city into `core/assets/noise_density.json` (34,137 cities) by
  `make_noise_density.py`, plus GeoNames population (already baked).
- **App.** A dotted violet curve on a fixed 45-90 dB(A) scale under the PM2.5
  curve; `Noise ~NN dB(A) est.` in the hover tooltip; a conditional legend
  entry; GRIP4 (ODbL) attribution in the location panel. **Show-only** - the
  verdict and best window are unchanged.
- **Tests.** core 26/26 (+4 noise), e2e 20/20 (+1 overlay, `on=321` / `off=0`);
  the frozen mock baseline is byte-identical (`AIRQLALITY_NOISE` defaults off
  under the mock seam).
- **Evidence.** `EXP16-noise.png`, `EXP16-noise-hover.png`, `EXP16-no-noise.png`,
  `EXP16-real-cairo.png`.
- **Not proven (out of scope).** It is an **estimate, not a measurement**, and
  city-level (one value per city; GRIP is 5 arcmin / ~8 km) so it cannot tell
  two streets apart. The diurnal shape is a documented generic profile, not
  measured traffic. Weekday/weekend and season are not modelled. No satellite or
  free live feed exists for Cairo.

**Recommended next: capture and analyse real audio.** The only route to a
*true, live, local* quiet-hours signal is to record ambient sound (an
A-weighted level over time, e.g. from the machine's microphone) and analyse it -
the separate app the user already has in mind. That replaces the modelled shape
with measured data and ground-truths this estimate. Named lesser follow-ups:
street-level OSM road-distance refinement, and a free-keyed live traffic API
for a data-driven diurnal shape.

## Learnings
- **GRIP4 road density is ESRI ASCII** (`.asc`), not GeoTIFF: skip its 6-line
  header and `np.fromstring(text, sep=" ")`. No GDAL, no Pillow. Sample the
  city's **own** 5-arcmin cell; a 3x3 mean dilutes dense cities at desert/coast
  edges (Cairo 12,677 vs 5,564 m/km^2).
- **Infer-noise level:** `28 + 8*log10(density+1) + 2*log10(pop+1)` dB(A),
  clamp 45-88, plus a documented diurnal profile (`core/src/noise.rs`); Cairo
  peak 74.8 / night 62.8.
- **New-overlay test seam:** `AIRQLALITY_NOISE=on|off`, default on in real mode
  but **off under `AIRQLALITY_SOURCE=mock`**, keeps historical e2e screenshots
  byte-identical; the new case opts in.
- **A legend swatch paints even when its curve is off** - make the footer legend
  conditional, or an e2e `off == 0` assert fails (8x8 px at 1.25 DPI = 100 px).
- **Footer is tight at 560 px** (EXP06): a 4th legend item collides with the
  axis key; shorten labels/strings. Tooltips wrap at `TOOLTIP_W = 150`.
- **Keep hour-of-day mapping in `core`** (`align_to_series`), pure and
  unit-tested, so `app` needs no `chrono` logic.
- Commands (MinGW on PATH first): gen
  `py -3.12 experiments/EXP16/make_noise_density.py`;
  `cargo test -p airqlality-core` (26/26);
  `cargo test -p airqlality --test e2e_widget` (20/20). Evidence: `capture.ps1`
  with mock + `AIRQLALITY_FREEZE_NOW=1` + `AIRQLALITY_NOISE=on` + an isolated
  `APPDATA`.
- **E2E screenshots flake under parallel load** (pre-existing): a case can
  capture a blank window; re-run in isolation.
