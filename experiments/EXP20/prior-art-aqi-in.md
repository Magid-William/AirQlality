# EXP20 prior art - aqi.in's Cairo historical page

Studied 2026-10-15 on <https://www.aqi.in/dashboard/egypt/al-qahirah/cairo/historical-analysis>
(screenshot: `EXP20-aqi-in-cairo.jpg`; the page refuses plain fetches with 403 and
was read in a real browser session).

## What it shows

Seven blocks: a 6-band AQI legend bar with a live marker · a two-year overlay line
chart (Oct 2025 in blue above Oct 2026 in green) · highest/lowest cards · **"same
day analysis"** (13 Oct: 2026 = 52 vs 2025 = 57) · **day/night 24 h trend** with
"avg highest time 3 AM / avg lowest time 2 PM" · a Jan-Dec calendar heatmap with
"218 Good / 54 Moderate / 11 Poor days" and "77 % within WHO's safe limits" ·
multi-pollutant tabs (AQI / PM2.5 / PM10 / CO / SO2 / NO2 / O3).

Its headline claim: **2026 average AQI (US) 44 vs 2025 average 68 = -35.2 %,
"improved"**; "most polluted year 2025", "least polluted 2026".

## The -35 % does not reproduce

Reproduce with the free, keyless CAMS archive on the same endpoint the app
already calls (Open-Meteo air quality accepts `start_date` / `end_date`):

```powershell
curl.exe -s "https://air-quality-api.open-meteo.com/v1/air-quality?latitude=30.0444&longitude=31.2357&start_date=2025-01-01&end_date=2026-10-14&hourly=pm2_5&timezone=Africa%2FCairo"
```

Means over all hours (US AQI from the 2024 PM2.5 breakpoints, the way IQAir
converts an instantaneous value):

| Series (Cairo) | PM2.5 µg/m³ | AQI (US) | Source |
|---|---|---|---|
| aqi.in historical page, 2025 | - | 68 | aqi.in |
| aqi.in historical page, 2026 (Jan-Oct) | - | 44 | aqi.in |
| CAMS archive, 2025 full year | 23.6 | 78 | this probe |
| CAMS archive, 2026 Jan-Oct 14 | 23.0 | 77 | this probe |
| CAMS archive, 2025 Jan-Oct 14 | 23.6 | 78 | this probe |
| CAMS archive, 2024 Jan-Oct 14 | 23.9 | 79 | this probe |
| IQAir station network, annual 2025 | 40.8 | 113 | IQAir (2024: 39.9 / 111) |

Like-for-like on CAMS: 1-13 Oct 2025 = 17.1 µg/m³ vs 1-13 Oct 2026 = 17.9 -
*slightly worse*, not 35 % better. Days whose 24 h mean is within the WHO 24 h
guideline (15 µg/m³): 6 % in 2025 Jan-Oct vs 8 % in 2026, against aqi.in's
"77 % of days within WHO's limits".

Their own numbers contradict each other too: their live Cairo dashboard read
AQI 107 / PM2.5 33 the same week, and their 24 h chart for 13-14 Oct 2026
(60, 62, 62, 74, 57, 66 overnight) matches CAMS hourly within ~10 %. The model
agrees with their *hourly* section and blows up only in the *yearly rollup*, so
the -35 % is an aggregation artifact, not air quality.

The one independent signal pointing the same way, and a modest one: Egypt's
Environment Ministry counted 486 rice-straw burning points 1 Sep - 7 Oct 2026 vs
647 in the same period of 2025 (-24.8 %, via EnterpriseAM, 9 Oct 2026).

**Lesson: a ratio with no window, no day count and no source label is
unfalsifiable dressing. Shape transfers between datasets; levels do not.**

## The shape *is* corroborated

CAMS hour-of-day means (2025-01-01 .. 2026-10-14, Cairo):

| hours | 00-05 | **06-08** | 12-14 | **15-17** | 18-23 |
|---|---|---|---|---|---|
| PM2.5 µg/m³ | 25.4-26.8 | **27.4-28.0** | 19.8-20.8 | **17.2-18.1** | 18.2-25.0 |

Month means: Jan worst (36.4 / 36.5), June best (20.4 / 16.4) - the same *month
ranking* aqi.in reports (their 2025: highest January, lowest June). Their levels
sit ~2x lower than CAMS and ~half of IQAir; their shape matches.

Episodes are separate from averages: CAMS Cairo on 14 Oct 2026 ran 15.7 µg/m³ at
15:00 and **46.5 at 21:00**, then decayed overnight - the documented black-cloud
mechanism (straw burned at night, smoke reaches the city after sunset, when the
temperature inversion forms; fires precede the haze by 6-9 h). The monthly mean
for October is one of the *lowest* of the year (19.4 / 18.1), so the "black
cloud" is episodic, not a monthly level: you cannot plan a window from a monthly
average, only from the next 24-48 h.

## UX inventory for a 560 px widget

Adopt:

- **Name the best and worst hour in words.** Their "highest at 3 AM, lowest at
  2 PM" is our app's whole job, said better than we said it.
- **Day vs Night as a first-class split** - matches the inversion physics and
  reads at widget size.
- **"Today vs your usual"** - their "same day analysis", minus the dishonest
  rollup: like-for-like, with the day count printed.
- **Their category counts** ("218 Good / 54 Moderate / 11 Poor days") - we take
  the *idea* (a census), not the six-band vocabulary or the unlabelled
  percentages.

Reject:

- 6-band `Good/Moderate/Poor/...` AQI vocabulary: our three verdict words are the
  differentiator (EXP01).
- Seven pollutant tabs - this app is PM2.5 and one decision.
- Annual calendar heatmap, "health risk level: Low", "77 % within WHO limits":
  decoration built on an aggregate that does not reproduce.
- The two-year overlay chart: beautiful, and it is exactly where the trap lives.
  If we ever ship a delta, it must name its window, its day count and its dataset
  (and for Cairo that delta is ~0 %, which is the honest answer).
