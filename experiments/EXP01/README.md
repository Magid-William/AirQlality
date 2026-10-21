# EXP01 - Prior art: data sources and products for "when to open the window"

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Discover and evaluate the prior art for "when is it safe to open my window in
Cairo?" — both the free/open **data sources** and the **competing products/tools**
— and decide whether a gpui Windows app is a genuine gap or a re-implementation.

## Hypothesis
Existing prior art is split: tools that show *raw AQI* (IQAir, aqicn, Windy) do
not give a clear open/close-window decision, while tools that *do* give that
decision are closed-source, mobile-only, or gated behind paid/account data.
Therefore a Cairo-first, Windows-native, free-open-data desktop tool with a
timeline is a real gap.

Falsification: if a free, open-source, Windows-capable, Cairo-relevant tool
already answers "open the window now vs. later", the hypothesis is false and the
project's premise needs rethinking.

## Methodology & blast radius
- Method: four passes, each producing rows in a matrix.
  1. Data sources — Open-Meteo AQ, OpenAQ v3, WAQI/aqicn, OpenWeatherMap, CAMS,
     IQAir. Record: key required?, license, Cairo coverage/nearest stations,
     pollutants, cadence, forecast horizon.
  2. Product prior art — IQAir AirVisual, Plume Labs, "Air Quality: Open or Vent
     Now", aqicn.org, AQI.in, AirGradient, Windy, Egypt EEAA. Record: platform,
     price, Cairo support, decision vs. number, open/closed.
  3. Open-source prior art — GitHub: ventilation/window decision tools, Home
     Assistant AQ automations/blueprints, gpui desktop apps on Windows.
  4. Decision logic — what thresholds/rules EPA/WHO/OSU-Extension recommend, and
     the counter-argument (indoor CO2 buildup argues *for* ventilating).
  Output: `experiments/EXP01/prior-art.md` (the matrix) plus a verdict.
- May touch: `experiments/EXP01/**`, `experiments-overview.md`.
- Off-limits: app code, `Cargo` project, locked dependency choices, edits to
  `AGENTS.md` or `UI.md`.

## Open questions (known unknowns)
- [x] Scope: data sources + products? — assumed **both**.
- [x] Bootstrap git? — assumed **yes**: `git init` on `main`, initial commit of
      `UI.md`/`AGENTS.md`, then branch `EXP01`.
- [x] Exhaustive or time-boxed? — assumed **time-boxed to saturation**, cap ~15.
- [x] Cairo-only or global? — assumed **Cairo-first**, global only if strong.
- [x] Probe APIs live? — assumed **one live no-key call** to Open-Meteo AQ Cairo.
- [x] Arabic/Egyptian sources? — assumed **best-effort yes**.

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.** The experiment answered its
question and settled the go/no-go: proceed, but narrower and repositioned.

Hypothesis **partially confirmed, partially falsified**. Full matrix:
`experiments/EXP01/prior-art.md`.

- Confirmed: mass-market tools (IQAir, aqicn, Windy, AQI.in) show numbers, not a
  decision; the decision logic exists only inside Home Assistant (needs hardware).
  Good forecast data is open via **Open-Meteo Air Quality — no key, CC BY 4.0**.
- **Falsified:** "no tool gives an open/close-window decision" is false — *Window Air*
  (iOS, "Air Quality: Open or Vent Now") already gives OPEN / VENT / KEEP CLOSED +
  best-window + timeline, free, no account.
- Still open: it is closed-source, **iOS-only, no Windows**, no automation/API, opaque
  rule. No open-source project covers Windows.

**Verdict: build, but narrower and repositioned.** Differentiators: (1) Windows-native
desktop, (2) a **transparent, documented rule**, (3) free open data, no account.
Ranked data sources: **Open-Meteo AQ (primary) > OpenAQ v3 / WAQI (ground truth,
key) > OpenWeatherMap (fallback)**.

**Risks / handover.** Differentiator is thin (platform + transparency, not a new
capability); US AQI is nearly flat for Cairo so the app must lead with PM2.5 + a time
window, not the headline number; no indoor CO2 sensor ⇒ the "vent" branch is
heuristic. **Next experiment:** gpui viability for a small Windows widget —
always-on-top overlay + tray on the fork path (Windows is not in the official `gpui`
crate; those features are the least-maintained).

## Learnings
- **Free Cairo AQ data, no key:** `air-quality-api.open-meteo.com/v1/air-quality?latitude=30.0444&longitude=31.2357&hourly=pm2_5,pm10,us_aqi,european_aqi&timezone=Africa/Cairo`
  (CC BY 4.0). The same provider serves wind via the free weather API — pair them to
  judge ventilation.
- OpenAQ v3 / WAQI give *observed* Cairo station values but need a free key and have
  no forecast — cross-check only, not for planning ahead.
- Prior-art baseline to beat: **Window Air** (iOS, closed source) already does
  OPEN/VENT/KEEP CLOSED + best-window; the gap is Windows + an open/transparent rule.
- Thresholds: Home Assistant's documented default is **PM2.5 ≤ 15 µg/m³**; WHO 2021
  24-h guideline **15 µg/m³** (annual 5).
- **gpui on Windows is the fork path** — official `gpui` targets macOS/Linux; Windows
  and always-on-top/tray come from community forks (`gpui-ce`, `adabraka-gpui`).
- Repo bootstrap for a new experiment: `git init -b main` → initial commit →
  `git checkout -b EXP0N`.
