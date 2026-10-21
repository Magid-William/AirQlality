# EXP19 - Stack three data sources and fuse them (median + disagreement)

## Summary
Add two more free sources behind `AirQualitySource` (a ground-station network and a
second global model), align them per hour, fuse by median, and surface per-source
values + an agreement signal so one bad/stale source can no longer drive the advice.

## Hypothesis
For Cairo, stacking >=2 independent sources (a) leaves the single-source widget
byte-identical when the new layer is off, (b) keeps the verdict stable when one
source is stale/an outlier (fault injection), and (c) shows at least one hour in a
24 h window where the sources straddle a verdict threshold - i.e. single-source data
was genuinely not enough, and the fused value + disagreement warning is measurably
more trustworthy than one model's number.

## Methodology & blast radius
- Method:
  1. **Probe (first):** with real keys, hit OpenAQ v3 + a second model for Cairo and
     record station count, freshness and distance from Open-Meteo. This picks the pair.
  2. **Core sources:** `core/src/openaq.rs` + `core/src/openweathermap.rs`, each a
     blocking `ureq` fetch plus a pure `parse_*` against a committed fixture (mirrors
     `openmeteo.rs`: injected clock, `SourceError`).
  3. **Core fusion (pure):** `core/src/fuse.rs` - hour-bucket each source, per-hour
     median + min/max spread + `sources_used`; conservative tie-break when the spread
     straddles a threshold; single-source fallback; stale/outlier rejection.
  4. **App wiring:** fetch enabled sources, fuse, paint. Per-source values + an
     "agree / sources disagree" chip; min-max envelope behind the PM2.5 line;
     per-source lines in the tooltip; header attribution lists the live sources.
     Keys from `%APPDATA%\AirQlality\config.json`, overridable by env
     (`AIRQLALITY_OPENAQ_KEY`, `AIRQLALITY_OWM_KEY`). No key -> that source is simply
     absent, and the app behaves exactly as before.
  5. **Tests:** parser fixtures; `fuse` unit tests (median, spread-straddle, stale,
     outlier, single-source, hour bucketing); fault injection; a new E2E mode
     (`AIRQLALITY_SOURCES=mock3`). `AIRQLALITY_SOURCE=mock` stays byte-identical so the
     existing suite (core 26 / app 31 / e2e 22) stays green.
- May touch: `core/src/{lib,source,openaq,openweathermap,fuse}.rs`,
  `core/tests/{fixtures/,live_*}.rs`, `app/src/{ui,persist}.rs`,
  `app/tests/e2e_widget.rs`, `core/assets/ATTRIBUTION.md`, `experiments/EXP19/`,
  `experiments-overview.md`.
- Off-limits: `decision.rs` threshold semantics; tray/icon/startup/notification code;
  other experiments; anything outside the repo.

## Open questions (known unknowns)
- [ ] What "stack" means - fuse or display-only? - proposed default: **fuse (median)
      drives the verdict, disagreement always visible**.
- [ ] Which two sources? - proposed default: **OpenAQ v3 (ground observations) +
      OpenWeatherMap (second hourly forecast)**; fall back to WAQI if the probe shows
      OpenAQ has no fresh Cairo station.
- [ ] Where do keys live? - proposed default: **`%APPDATA%\AirQlality\config.json` +
      env overrides; no key => source absent, no regression**.
- [ ] Disagreement rule - proposed default: **conservative** (band straddling a
      threshold picks the more protective verdict, labelled "sources disagree").
- [ ] Future has no ground source - proposed default: **say so** ("forecast: model
      only; now: N sources"), never hide it.
- [ ] Bias-correct the forecast to the ground reading? - proposed default: **no silent
      shift**; show the observed-vs-modelled delta only.
- [ ] Fetch sequencing - proposed default: **parallel threads inside the one
      background task**, short per-source timeout.
- [ ] Staleness - proposed default: **drop a source hour older than 2 h**; if all
      ground sources are stale, model-only with a note.
- [ ] UI space - proposed default: **one ~16 px sources line that renders only when >1
      source is active**, and the envelope only in multi-source mode.

_Assumptions adopted (user did not answer): the proposed defaults above._

## How to enable the new sources

Open-Meteo stays keyless. The other two are optional and simply absent without a
key, so the app never regresses:

- Create `%APPDATA%\AirQlality\config.json`:

  ```json
  { "openaq_key": "…", "owm_key": "…" }
  ```

  - `openaq_key` — free account at <https://explore.openaq.org/register>.
  - `owm_key` — free account at <https://openweathermap.org/api>.

- Or set `AIRQLALITY_OPENAQ_KEY` / `AIRQLALITY_OWM_KEY` (they win over the file).

Then run the live probe to see what Cairo actually gives:

```text
cargo test -p airqlality-core --test live_sources live_cairo_stack_report -- --ignored --nocapture
```

## Live probe (2026-10-15, Cairo)

Run with both keys set (`cargo test … live_cairo_stack_report -- --ignored --nocapture`):

```text
open-meteo   now 2026-10-15 13:00 +0300 = 30.2 ug/m3 (96 points)
skipped: OpenAQ: source unavailable: no fresh PM2.5 station within 25 km
         (nearest: Walkthru Earth Main at 38 km)
skipped: OpenWeatherMap: network error: …appid=***: status code 401

=== Cairo stack, 2026-10-15 13:00:00 +03:00 ===
  Open-Meteo         30.2  (Model)
  fused              30.2   spread 30.2-30.2 across 1 source(s)
  verdict          KEEP CLOSED
  best window      03:00 - 21:00 (19 h)
```

What the probe established:

- **OpenAQ has no usable Cairo ground station.** A country-wide `bbox` sweep finds
  five Egyptian PM2.5 stations: two "Cairo" monitors (AirNow / US Public Domain,
  last reported **2025-03-04** and **2023-09-25** - the embassy programme is dead)
  and one fresh citizen-grade sensor, **"Walkthru Earth Main"**, ~**38 km** east of
  the city. OpenAQ caps `radius` at 25 km, so the reference monitor is unreachable
  *and* would be stale anyway.
- **The fusion refused to use it**, by design: `MAX_KM = 25` means a station that
  far away is reported as unavailable rather than fused in as "ground truth". The
  app shows that reason in its stale note, so the stack never silently shrinks.
- **OpenWeatherMap rejected the key with 401** at first use (new OWM keys take up
  to ~2 h to activate), so that leg of the stack is still unverified live.
- **A secret leak was found and fixed:** `ureq` puts the request URL in its
  transport errors and OWM carries the key as a query parameter, so the raw error
  text would have printed the user's API key into the widget's "offline" note and
  into any screenshot. `http::get_redacted` now scrubs every key from all error
  strings (unit-tested; visible as `appid=***` in the probe output above).

Net for Cairo today: the stack is **1 model + 0 stations**, and the UI says so
rather than pretending. WAQI/aqicn is the candidate second *ground* source
(needs its own free token) if a real Cairo station is wanted.

### What the real app shows (EXP19-real.png)

Running the release path with both keys set, no mock:

```text
KEEP CLOSED            data © Open-Meteo · Cairo, Egypt
PM2.5 30 µg/m³ now · best window 03:00 - 21:00 (19 h)
offline — showing last data · OpenWeatherMap: network error: HTTP 401
```

Three deliberate behaviours are visible there:

- **One live source means no stack chrome**: no sources strip and no envelope,
  because there is nothing to stack. The header counts what is actually live.
- **A coverage problem is not an outage.** OpenAQ's "no fresh station within
  25 km" is dropped from the note: it will not fix itself, and repeating it every
  launch is noise. A **network/parse** failure (`HTTP 401`) *is* an outage, so it
  is reported.
- **HTTP failures are reported as status codes**, not URLs, which is both
  readable and the smallest surface a key could ever leak from.

## Conclusion (findings)
_Pending._

## Learnings
- OpenAQ v3 wire facts: the parameter is **`pm25`** (not `pm2_5`); an hour's
  timestamp is **`period.datetimeFrom`**, not a `datetime` field; `radius` is
  capped at **25000 m**; `bbox` is `minLon,minLat,maxLon,maxLat`; `/hours`
  returns the station's **oldest** hours, so pass `datetime_from` to window it.
  Parse against the real payloads, never against a guessed shape.
- `curl.exe -sI https://data.sensor.community/static/v2/data.1h.json` then parse the
  dump - use when checking keyless sensor-network coverage (2026-10-15: 18,185 entries,
  **0 in Egypt**, so Sensor.Community is unusable for Cairo).
- OpenAQ v2 is **410 Gone**, v3 is **401 without a key** - use v3 with `X-API-Key`.
- **Never put a secret in an error string**: `ureq` includes the URL, and OWM takes
  its key as a query parameter, so scrub it (`core/src/http.rs::scrub`). Check any
  error path a user can see (the widget's stale note is one).
- Probe with `curl.exe -s -H "X-API-Key: $env:KEY" …` and pipe the *body* only;
  mask keys in shell output (`.Replace($env:KEY,'***')`) before reading it.
- A new OpenWeatherMap key returns **401 until it activates** (minutes to ~2 h);
  treat a first-use 401 as "not yet", not as "bad key".
- `cargo test -p airqlality-core` / `-p airqlality --bin airqlality` /
  `--test e2e_widget` with `C:\Users\MW\AppData\Local\mingw64\bin` prepended to
  `PATH` - the whole offline suite, no network, no keys.
- The widget opens **centred**, so a physical mouse left mid-screen sits over it:
  Windows then posts a real `WM_MOUSEMOVE`, the hover tooltip appears (its `PANEL`
  pixels, plus the crosshair) and the "dropdown closed" / "no hover" pixel
  assertions fail **flakily** depending on where the user's mouse is.
  `capture.ps1` now parks the pointer at `(0,0)` before opening the window; the
  hover test is unaffected (it posts its own synthetic `WM_MOUSEMOVE`).
- A stray `airqlality.exe` - the capture script's `Kill` can miss a tray-hidden
  instance - locks `target\debug\airqlality.exe` and `cargo test` then dies with
  "failed to remove file". `Get-Process airqlality | Stop-Process -Force` first.
- A new UI layer that must not move the old screenshots: gate it behind its own env
  seam (`AIRQLALITY_SOURCES=mock3`) and assert the default run paints **zero** of the
  new colours - that is what keeps the exact-pixel E2E counts honest.
- Live keyed smoke tests go in `core/tests/live_sources.rs`, `#[ignore]`d and gated on
  `AIRQLALITY_OPENAQ_KEY` / `AIRQLALITY_OWM_KEY` so the offline suite needs no keys.
  Keep one **report** test that never fails (`live_cairo_stack_report`): it prints the
  per-source values and the fusion, which is the evidence a live probe is for.
