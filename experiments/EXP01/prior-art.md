# EXP01 — Prior art matrix

Research date: 2026-10-04. Scope: Cairo-first. Sources: live API probe + web search
(see `README.md`). This file is the evidence behind the verdict.

## 1. Data sources (free/open first)

| Source | Key? | License / terms | Cairo coverage | Variables | Cadence | Forecast | Verdict |
|---|---|---|---|---|---|---|---|
| **Open-Meteo Air Quality** | **No key** | CC BY 4.0 data; 10k calls/day free, non-commercial | Global model grid (~11 km), covers Cairo (probed: 30.0/31.2) | PM2.5, PM10, NO2, O3, SO2, CO, + US & EU AQI, pollen, dust | Hourly | up to 7 days | **Primary pick** |
| OpenAQ v3 | Yes (free account) | Free key, 60/min, 2000/hr; one key per person | Station-based (US Embassy + others) | Whatever the station reports | Hourly obs | none | Ground-truth cross-check |
| WAQI / aqicn.org | Yes (free token) | Limited free JSON API | Cairo US Embassy station | PM2.5 etc. | Real-time | 3-day | Ground-truth Cairo station |
| OpenWeatherMap Air Pollution | Yes (free tier) | Free tier, key | Global, coarse | CO, NO, NO2, O3, SO2, PM2.5, PM10, NH3, AQI | Hourly | ~4 days | Fallback only |
| CAMS (Copernicus) | No (GADS) | CC BY, free | Global | Full suite | Hourly | ~5 days | Upstream of Open-Meteo; heavier to consume |
| IQAir AirVisual API | Yes (paid) | Commercial, freemium tiers | Global + sensors | Full suite | Real-time | Yes | Not free — reject |
| Google Air Quality API | Yes (paid) | Commercial | Global | Full suite | Hourly | Yes | Not free — reject |

Notes: Open-Meteo AQ is itself a friendly front-end over CAMS (Europe) + CAMS global,
so it is a legitimate "open data" path. OpenAQ/WAQI are *observations*, not forecasts —
good for verifying the model, bad for planning ahead. No source gives an
*indoor/CO2* signal; that needs a local sensor, which this app will not have.

## 2. Product prior art

| Product | Platform | Price | Gives open/close **decision**? | Data | Open source? | Cairo? | Notes |
|---|---|---|---|---|---|---|---|
| **Window Air** ("Air Quality: Open or Vent Now") | **iOS 17+ only** | Free + IAP | **Yes** — OPEN / VENT / KEEP CLOSED + "Best Window Today" + verdict timeline | "public environmental data", no account | **No** | Yes (global) | **Closest competitor.** Solo dev (JUHWAN PARK), 2026. No Windows, no API, rule not transparent. |
| IQAir AirVisual | iOS/Android/Web | Free + IAP | No — number + improving/worsening alerts | Own sensors + public | No | Yes | Market leader; number-first |
| Plume Labs (Air Report / World Air Map) | iOS/Android/Web | Free | Partial — "personalized advice", number-first | Data fusion | No | Yes (has Cairo page) | Flow/Flow 2 hardware discontinued |
| aqicn.org / WAQI | Web + apps | Free | No — number | Gov + US Embassy stations | No (site) | Yes | US Embassy Cairo real-time |
| AQI.in / AQI Air | Web + apps | Free | No — number | Public feeds | No | Yes | Marketing-heavy |
| Windy | Web/apps/desktop | Free + premium | No — map overlay | CAMS etc. | No | Yes | Desktop exists; not a decision tool |
| AirGradient Map | Web + apps | Free | No — number | Own + public monitors | Open hardware/firmware | Sparse | Needs their hardware |
| EPA AirNow | iOS/Android | Free | No — number + forecast | US network | Public data | **No** | US only |
| Egypt EEAA | Reports / own network | n/a | No | 42-site national network (NILU) | No live API found | Yes | Not app-consumable |

## 3. Open-source / automation prior art

| Project | What it does | Gap for us |
|---|---|---|
| Home Assistant core `air_quality.*` triggers | Native PM2.5 value/changed triggers; docs ship an example automation *"Open covers only if air is clean"* (threshold 15 µg/m³) | Requires HA + physical sensors + setup |
| `KadenThomp36/air-quality-card` (GitHub) | HA dashboard card with actionable "Open Window" / "Run Air Purifier" | Same HA dependency; trades a *number* endpoint |
| AirGradient | Open-source monitor + dashboard | Hardware-gated |
| Breeze AQWidget (EUPL) | EU air-quality info widget | Display-only, not a window decision |
| `awesome-gpui` app list | DB clients, terminals, launchers, monitors | **No air-quality / window-decision app exists in the gpui ecosystem** |

Searched and **not found**: an open-source, Windows-native, no-account desktop tool
that turns free AQ data into an open/close-window decision.

## 4. Decision logic prior art (what the thresholds actually are)

- **WHO 2021** (PM2.5): annual guideline **5 µg/m³**, 24-hour **15 µg/m³**; interim
  targets 10/25, 15/37.5, 25/50. "No known safe threshold — lower is better."
- **US EPA / extension guidance**: open windows when AQI is green/yellow; keep closed
  above ~AQI 100.
- **Home Assistant community default**: close/reject ventilation when outdoor PM2.5
  is above ~**15 µg/m³**; some users use ~20.
- **Counter-argument** (important): opening windows raises indoor PM but **lowers
  indoor CO2/VOC**. So a truthful recommendation is two-sided — *vent briefly even
  when outside air is mediocre* — which is exactly the "VENT" nuance in Window Air.

## 5. Live Cairo probe (Open-Meteo, 2026-10-04, no key)

Request: `air-quality-api.open-meteo.com/v1/air-quality?latitude=30.0444&longitude=31.2357&hourly=pm10,pm2_5,...,us_aqi,european_aqi&forecast_days=2&timezone=Africa/Cairo`

Observed over 48 h:
- PM2.5 **14–26 µg/m³**, US AQI **68–73**, EU AQI **43–61** — i.e. "moderate".
- PM10 peaks (~99) late afternoon; NO2 peaks in the evening (~62); SO2 spikes at
  midday on the weekday (~125).
- The US AQI band is nearly flat (68–73) while EU AQI and pollutants move — **a single
  number hides meaningful intra-day structure. A timeline genuinely adds information.**

## 6. Verdict

**Hypothesis: partially confirmed, partially falsified → build, but narrower and
repositioned.**

- *Confirmed:* mass-market tools (IQAir, aqicn, Windy, AQI.in) show **numbers, not a
  decision**; the real decision logic lives only inside Home Assistant, which needs
  hardware and setup. Good data (HQ forecasts) is open via Open-Meteo with no key.
- *Falsified:* the central claim — "no tool gives an open/close-window decision" — is
  **false**. *Window Air* already does OPEN/VENT/KEEP CLOSED + best-window + timeline,
  free, no account, globally.
- *What is still genuinely open:* it is **closed-source, iOS-only, has no Windows
  build, no automation/API, and its rule is opaque**. No open-source project covers
  Windows.

**Differentiator to build around (not "we show AQI"):**
1. **Windows-native desktop** — the one platform none of the decision tools serve.
2. **Transparent, documented rule** — publish the threshold logic (WHO/EPA + the
   vent-for-CO2 counter-argument) as a feature, not a black box.
3. **Free, open data, no account** — Open-Meteo AQ, no key, cacheable offline.

**Ranked data source:** 1) Open-Meteo AQ (primary), 2) OpenAQ v3 / WAQI (ground-truth
cross-check, key required), 3) OpenWeatherMap (fallback).

**Risks / caveats**
- The differentiator is **thin**: platform + transparency, not a new capability. This
  is a lifestyle utility, not a moat.
- `us_aqi` is nearly flat for Cairo; the product must lead with PM2.5 + a time-window
  recommendation, not the headline AQI number.
- No indoor CO2 sensor ⇒ the "vent" branch must be heuristic, stated as such.
- **gpui viability risk (out of scope here):** Zed has deprioritized gpui work not
  needed by Zed itself, and the community fork `gpui-ce` reportedly has little
  activity. Windows rendering works (Win32 + DirectWrite), but the framework's future
  is uncertain. Recommend a dedicated experiment before committing the whole app.
