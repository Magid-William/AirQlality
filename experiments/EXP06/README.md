# EXP06 - Real data: `OpenMeteoSource` behind `AirQualitySource`

**Status:** `success` (user-declared, 2026-10-04).

## Summary
Replace `MockSource` at runtime with a real `OpenMeteoSource` behind the existing
`AirQualitySource` trait: HTTPS fetch + parse into a real `Series`, "now" driven by
the real clock, periodic refresh, and honest loading/error states — so the widget
shows today's actual Cairo air.

## Hypothesis
Open-Meteo AQ can be integrated behind the existing trait with a small blocking
HTTPS client and `serde_json`, so the app renders a real Series with `now` = the
real current hour, refreshes on a timer, and shows a clear loading/error state on
failure — while the mock path stays byte-identical so the EXP04/05 E2E still passes.

**Falsified if:** HTTPS/TLS won't build or run on the GNU toolchain, the parsed
Series can't be derived deterministically from a fixture, the real-clock path can't
keep the E2E deterministic, or error/loading states can't be rendered.

## Methodology & blast radius
- Method:
  1. **De-risk first (riskiest part).** Add `ureq` (blocking, `native-tls` ->
     Windows **schannel**, no C toolchain) + `serde`/`serde_json`, run a throwaway
     live GET, and capture the response as a committed fixture. If ureq/TLS fights
     MinGW, fall back to **WinHTTP via the already-present `windows` crate**.
  2. **`core`.** New `core/src/openmeteo.rs`: `OpenMeteoSource` implementing
     `AirQualitySource`; a **pure** `parse_openmeteo(json, now) -> Result<Series,
     SourceError>`; extend `SourceError` with `Network`/`Parse`. `model`/`decision`
     stay pure — only this module does I/O.
  3. **`core` tests** against `core/tests/fixtures/openmeteo-cairo.json`: point
     count; times carry the `utc_offset_seconds` offset; `now_index` for an
     injected now (mid / before-start / after-end clamps); null `pm2_5` hours
     skipped; malformed JSON -> `Parse` error.
  4. **`app`.** A `Load { Loading, Ready(Series, Decision), Failed(String) }`
     state; `Widget::new` kicks an async background fetch (no UI-thread block);
     30 min refresh; render loading/error; keep last-good on a mid-session failure
     with an "offline/stale" note; footer attribution.
  5. **Test seam.** `AIRQLALITY_SOURCE=mock` (unset -> Open-Meteo); E2E sets `mock`
     + `AIRQLALITY_FREEZE_NOW` -> screenshot stays byte-identical. Optional
     `AIRQLALITY_OPENMETEO_URL` override forces a deterministic error for an
     error-banner assertion (bite-proof).
  6. **Evidence.** A live screenshot on real Cairo data
     (`experiments/EXP06/EXP06-real.png`), the fixture, and the gated live-test
     log. Verify: `cargo test -p airqlality-core`,
     `cargo test -p airqlality --test e2e_widget`, release run + capture.
- May touch: `Cargo.toml`/`Cargo.lock`, `core/Cargo.toml`, `core/src/{lib,source}.rs`,
  `core/src/openmeteo.rs` (new), `core/tests/**`, `app/src/{main,ui}.rs`,
  `app/tests/e2e_widget.rs`, `experiments/EXP06/**`, `experiments-overview.md`,
  branch `EXP06` (from `80847a5`).
- Off-limits: `UI.md`, `AGENTS.md`, `experiments/EXP01..05/**` (frozen evidence),
  the rule thresholds/logic in `decision.rs`, declaring a final status.
- Machine-level blast radius: adds `ureq`/`native-tls` compile time (schannel, no C);
  makes real network calls during dev; captures one real window.

## Open questions (known unknowns)
- [x] HTTP/TLS client — **`ureq` blocking + `native-tls`** (schannel; no C
      compiler); fallback WinHTTP via `windows` (plan default).
- [x] Where the I/O lives — **`core/src/openmeteo.rs`** (EXP03 predicted the source
      swap touches only the source); `model`/`decision` stay pure (plan default).
- [x] Source seam — **env `AIRQLALITY_SOURCE`** (`mock` -> mock; unset -> Open-Meteo)
      (plan default).
- [x] Clock — **real current hour** for the real source; 3 s accelerated ticker
      stays **mock-only** (plan default).
- [x] Refresh cadence — **every 30 min** (~48 calls/day) (plan default).
- [x] Failure UX — first-load failure -> **error banner + auto-retry**; mid-session
      failure -> **keep last-good + stale note** (plan default).
- [x] Wind — **don't request it** (API returns nulls); set `0.0`, verdict
      unaffected (plan default).
- [x] Horizon/params — **lat 30.0444, lon 31.2357, tz Africa/Cairo,
      `past_days=1&forecast_days=2`, `timeformat=unixtime`** (plan default).
- [x] Tests/fixture — **committed JSON fixture + pure `parse` unit tests**; live
      fetch behind an `#[ignore]`/env gate as manual evidence (plan default).
- [x] Disk cache — **none this experiment** (in-memory last-good only) (plan default).
- [x] Attribution — **"Data (c) Open-Meteo (CC BY 4.0)"** in the footer (plan default).
- [x] Scope — **one experiment** (fetch + parse + real now + states + refresh)
      (plan default).
- [x] Base branch — **`EXP06` from `80847a5`** (plan default).

## Conclusion (findings)
**Concluded `success` by the user on 2026-10-04.**

**Hypothesis upheld.** Open-Meteo Air Quality integrates behind the existing
`AirQualitySource` trait, and the widget now runs on **real Cairo data** with the
real current hour, a 30 min refresh, and honest loading/error screens — while the
mock path stayed byte-identical so the EXP04/05 E2E still pins the rule/rendering.

- **Real source (`core/src/openmeteo.rs`).** Blocking HTTPS via `ureq` +
  `native-tls` (**schannel**, no C toolchain on GNU/MinGW) + `serde_json`. The
  response parsing is a **pure** `parse_openmeteo(json, now)`; `model`/`decision`
  remain pure, so the only I/O in `core` is this module. `SourceError` gained
  `Network`/`Parse`.
- **DST-safe time, no tz database.** `timeformat=unixtime` + the response's
  `utc_offset_seconds` as a `FixedOffset` (Oct 2026 Cairo = **+03:00**, winter
  +02:00). `now_index` is derived from the real clock via `partition_point`.
- **Live proof.** Ignored network test returned **96 points, `now_index` 42**;
  offline unit suite is **12/12** (fixture parse, now-index clamps, null-hour
  skipping, malformed JSON).
- **App.** Real source by default; `Load` on real data with fetch off the UI
  thread, re-evaluate + repaint, **30 min refresh**; `Loading` / `Ready` /
  `Failed`; a mid-session failure keeps last-good data + a stale note; header
  attribution "data © Open-Meteo".
- **Seams.** `AIRQLALITY_SOURCE=mock` (deterministic mock + accelerated ticker)
  and `AIRQLALITY_OPENMETEO_URL` (force an error). Mock E2E verdict counts
  unchanged (`closed=2043 vent=6293 open=790 top_closed=1131`); a second E2E
  forces the error screen. App E2E **2/2**.
- **Evidence:** `experiments/EXP06/EXP06-real.png` (live **VENT**, PM2.5 20,
  best window 11:00–18:00, 8 h) and `EXP06-error.png` (forced refusal); fixture
  `core/tests/fixtures/openmeteo-cairo.json`. Release = **27.0 MB** (was 26.6).
- **Not proven (out of scope):** no disk cache (in-memory last-good only); the
  stale-note path has no screenshot; the error screen prints the full request URL
  (dense); wind is `0.0` (the AQ API returns no wind) and still does not affect
  the verdict; the location is a constant (no settings).

**Recommended next:** cache the last good response to disk (survive restarts
offline), then the always-on-top + tray frontier EXP01 flagged.

## Learnings
- **HTTPS with ureq on Windows/GNU:** its `native-tls` feature only *provides* a
  `TlsConnector` impl — you must wire it:
  `AgentBuilder::new().timeout(..).tls_connector(Arc::new(native_tls::TlsConnector::new()?)).build()`
  with `native-tls = "0.2"` as a direct dep (schannel, no C). Otherwise HTTPS
  fails with *"no TLS backend is configured"*.
- **Commands:** offline `cargo test -p airqlality-core` (12/12) +
  `cargo test -p airqlality --test e2e_widget` (2/2); live network proof
  `cargo test -p airqlality-core --test live_openmeteo -- --ignored --nocapture`.
- **Time:** request `timeformat=unixtime` and attach `utc_offset_seconds` as a
  `FixedOffset` — DST-correct without a tz database.
- **Open-Meteo AQ has no wind** (`wind_speed_10m` all null) → `wind_speed = 0.0`.
- **Window-screenshot E2E tests race** (same centred position, `CopyFromScreen`);
  serialize them behind a `static Mutex<()>`.
- **Adding deps shifts the E2E `bg` count** (text layout) but not the verdict
  counts → assert floors, not the exact `bg`.
- **Attribution** belongs in the header; in the footer it collides with the legend
  at 560 px.
