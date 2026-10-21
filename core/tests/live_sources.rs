//! Live network smoke tests for the stacked sources (EXP19). Ignored by default
//! so the offline suite stays deterministic and needs no keys.
//!
//! Run them manually with real keys:
//!
//! ```text
//!   $env:AIRQLALITY_OPENAQ_KEY = "…"
//!   $env:AIRQLALITY_OWM_KEY = "…"
//!   cargo test -p airqlality-core --test live_sources -- --ignored --nocapture
//! ```

use airqlality_core::{
    AirQualitySource, Kind, Location, OpenAqSource, OpenMeteoSource, OpenWeatherMapSource,
    SourceSeries, Thresholds, stack,
};

fn assert_well_formed(series: &airqlality_core::Series, what: &str) {
    eprintln!(
        "live {what}: {} points, now_index {}, location {}",
        series.len(),
        series.now_index,
        series.location
    );
    assert!(series.len() >= 6, "{what}: expected at least 6 hours");
    assert!(series.now_index < series.len(), "{what}: now_index in range");
    assert!(
        series.points.iter().all(|p| p.pm2_5 >= 0.0),
        "{what}: PM2.5 must be non-negative"
    );
    assert!(
        series
            .points
            .windows(2)
            .all(|w| (w[1].time - w[0].time).num_minutes() > 0),
        "{what}: points must ascend"
    );
}

#[test]
#[ignore = "hits the real OpenAQ API; needs AIRQLALITY_OPENAQ_KEY"]
fn live_openaq_fetch_is_well_formed() {
    let key = std::env::var("AIRQLALITY_OPENAQ_KEY")
        .expect("set AIRQLALITY_OPENAQ_KEY to run this test");
    let source = OpenAqSource::for_location(&Location::cairo(), &key).expect("build source");
    let series = source.fetch().expect("live OpenAQ fetch");
    assert_well_formed(&series, "OpenAQ");
}

#[test]
#[ignore = "hits the real OpenWeatherMap API; needs AIRQLALITY_OWM_KEY"]
fn live_openweathermap_fetch_is_well_formed() {
    let key = std::env::var("AIRQLALITY_OWM_KEY")
        .expect("set AIRQLALITY_OWM_KEY to run this test");
    let source =
        OpenWeatherMapSource::for_location(&Location::cairo(), &key).expect("build source");
    let series = source.fetch().expect("live OpenWeatherMap fetch");
    assert_well_formed(&series, "OpenWeatherMap");
}

/// The experiment's evidence run: fetch every source Cairo can give, print what
/// each reported, and print the fusion. **Never fails** on a missing or unusable
/// source — the point is the report, not a green tick. Needs network; keys are
/// optional (a missing one is reported and skipped).
///
/// ```text
///   cargo test -p airqlality-core --test live_sources live_cairo_stack_report -- --ignored --nocapture
/// ```
#[test]
#[ignore = "live probe: prints the real Cairo stack (network; keys optional)"]
fn live_cairo_stack_report() {
    let location = Location::cairo();
    let mut sources: Vec<SourceSeries> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // Open-Meteo: keyless, always attempted.
    match OpenMeteoSource::for_location(&location) {
        Ok(source) => match source.fetch() {
            Ok(series) => {
                eprintln!(
                    "open-meteo   now {} = {:.1} ug/m3 ({} points)",
                    series.now().time.format("%Y-%m-%d %H:%M %z"),
                    series.now().pm2_5,
                    series.len()
                );
                sources.push(SourceSeries {
                    id: "open-meteo",
                    label: "Open-Meteo",
                    kind: Kind::Model,
                    series,
                });
            }
            Err(e) => notes.push(format!("Open-Meteo: {e}")),
        },
        Err(e) => notes.push(format!("Open-Meteo: {e}")),
    }

    // OpenAQ: ground stations, needs a key.
    match std::env::var("AIRQLALITY_OPENAQ_KEY") {
        Ok(key) => match OpenAqSource::for_location(&location, &key) {
            Ok(source) => match source.fetch() {
                Ok(series) => {
                    eprintln!(
                        "openaq       now {} = {:.1} ug/m3 ({} points)  [{}]",
                        series.now().time.format("%Y-%m-%d %H:%M %z"),
                        series.now().pm2_5,
                        series.len(),
                        source.stations_url()
                    );
                    sources.push(SourceSeries {
                        id: "openaq",
                        label: "OpenAQ",
                        kind: Kind::Ground,
                        series,
                    });
                }
                Err(e) => notes.push(format!("OpenAQ: {e}")),
            },
            Err(e) => notes.push(format!("OpenAQ: {e}")),
        },
        Err(_) => notes.push("OpenAQ: no AIRQLALITY_OPENAQ_KEY".to_string()),
    }

    // OpenWeatherMap: second model, needs a key.
    match std::env::var("AIRQLALITY_OWM_KEY") {
        Ok(key) => match OpenWeatherMapSource::for_location(&location, &key) {
            Ok(source) => match source.fetch() {
                Ok(series) => {
                    eprintln!(
                        "openweather  now {} = {:.1} ug/m3 ({} points)",
                        series.now().time.format("%Y-%m-%d %H:%M %z"),
                        series.now().pm2_5,
                        series.len()
                    );
                    sources.push(SourceSeries {
                        id: "openweathermap",
                        label: "OpenWeatherMap",
                        kind: Kind::Model,
                        series,
                    });
                }
                Err(e) => notes.push(format!("OpenWeatherMap: {e}")),
            },
            Err(e) => notes.push(format!("OpenWeatherMap: {e}")),
        },
        Err(_) => notes.push("OpenWeatherMap: no AIRQLALITY_OWM_KEY".to_string()),
    }

    for note in &notes {
        eprintln!("skipped: {note}");
    }
    assert!(
        !sources.is_empty(),
        "not even Open-Meteo answered; check the network"
    );

    let stack = stack(sources, &Thresholds::default()).expect("fuse what came back");
    let now = stack.series().now();
    eprintln!("\n=== Cairo stack, {} ===", now.time);
    for reading in stack.readings_now() {
        eprintln!(
            "  {:<16} {:>6}  ({:?})",
            reading.label,
            reading
                .pm2_5
                .map(|v| format!("{v:.1}"))
                .unwrap_or_else(|| "-".into()),
            reading.kind
        );
    }
    let spread = stack.now_spread();
    eprintln!(
        "  fused            {:>6.1}   spread {:.1}-{:.1} across {} source(s){}",
        now.pm2_5,
        spread.min,
        spread.max,
        spread.count,
        if stack.now_disagrees() {
            "   <-- SOURCES DISAGREE (straddles a threshold)"
        } else {
            ""
        }
    );
    eprintln!(
        "  verdict          {}  ({})",
        stack.decision.headline.label(),
        stack.decision.headline.advice()
    );
    if let Some(window) = stack.decision.best_window {
        let s = stack.series();
        eprintln!(
            "  best window      {} - {} ({} h)",
            s.points[window.start].time.format("%H:%M"),
            s.points[window.end].time.format("%H:%M"),
            window.hours()
        );
    }
}

/// The evidence run behind EXP20's shape claim: print the CAMS archive's hour-of-
/// day and month means for Cairo, plus the like-for-like year comparisons the
/// aqi.in study needed. Keyless. **Never fails** on a network error - the point
/// is the report, not a green tick.
///
/// Why this data: the app's advice is only as good as its picture of the day, and
/// the archived CAMS hours are the only free, multi-year, city-level series Cairo
/// has (OpenAQ has no fresh station within 25 km, see `live_cairo_stack_report`).
///
/// ```text
///   cargo test -p airqlality-core --test live_sources live_cairo_history_report -- --ignored --nocapture
/// ```
#[test]
#[ignore = "live probe: prints Cairo's CAMS archive (network; keyless)"]
fn live_cairo_history_report() {
    let location = Location::cairo();
    // Two full years back from yesterday: enough for a like-for-like year over
    // year comparison, and the archive answers one request this size.
    let today = chrono::Local::now().date_naive();
    let start = today - chrono::Duration::days(2 * 365 + 1);
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}&longitude={}\
         &start_date={}&end_date={}&hourly=pm2_5&timezone={}",
        location.lat,
        location.lon,
        start.format("%Y-%m-%d"),
        today.format("%Y-%m-%d"),
        location.tz.replace('/', "%2F")
    );

    let body = match probe_get(&url) {
        Ok(body) => body,
        Err(e) => {
            eprintln!("live history probe skipped: {e}");
            return;
        }
    };
    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("live history probe skipped: unparseable body: {e}");
            return;
        }
    };
    let (Some(times), Some(values)) = (
        json["hourly"]["time"].as_array(),
        json["hourly"]["pm2_5"].as_array(),
    ) else {
        eprintln!("live history probe skipped: unexpected payload shape");
        return;
    };
    let hours: Vec<(&str, f32)> = times
        .iter()
        .zip(values)
        .filter_map(|(t, v)| Some((t.as_str()?, v.as_f64()? as f32)))
        .collect();
    assert!(
        hours.len() > 24 * 300,
        "expected at least a year of hours, got {}",
        hours.len()
    );
    eprintln!(
        "Cairo archive: {} hours, {} .. {} (source: CAMS via Open-Meteo)",
        hours.len(),
        hours[0].0,
        hours[hours.len() - 1].0
    );

    // Month means: the seasonal shape (winter worst, June cleanest in Cairo).
    let mut months: Vec<(String, Vec<f32>)> = Vec::new();
    for (time, value) in &hours {
        let month = time[..7].to_string();
        match months.last_mut() {
            Some((last, values)) if *last == month => values.push(*value),
            _ => months.push((month, vec![*value])),
        }
    }
    eprintln!("\nmonth      mean PM2.5   mean AQI (US)");
    for (month, values) in &months {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        eprintln!("  {month}   {:>7.1} µg/m³   {:>5.0}", mean, us_aqi(mean));
    }

    // Hour-of-day means: the shape the app's advice rests on.
    let mut by_hour: [Vec<f32>; 24] = Default::default();
    for (time, value) in &hours {
        let hour: usize = time[11..13].parse().expect("ISO hour");
        by_hour[hour].push(*value);
    }
    let hour_mean = |h: usize| {
        let values = &by_hour[h];
        values.iter().sum::<f32>() / values.len() as f32
    };
    let worst = (0..24)
        .max_by(|a, b| hour_mean(*a).total_cmp(&hour_mean(*b)))
        .unwrap();
    let best = (0..24)
        .min_by(|a, b| hour_mean(*a).total_cmp(&hour_mean(*b)))
        .unwrap();
    eprintln!("\nhour  mean PM2.5     hour  mean PM2.5");
    for h in 0..12 {
        eprintln!(
            "  {h:02}     {:>6.1}        {:02}     {:>6.1}",
            hour_mean(h),
            h + 12,
            hour_mean(h + 12)
        );
    }
    eprintln!(
        "worst hour {worst:02} = {:.1} µg/m³, best hour {best:02} = {:.1} µg/m³",
        hour_mean(worst),
        hour_mean(best)
    );

    // The like-for-like comparisons from the prior-art study (EXP20): a year over
    // year claim is only meaningful over the same calendar window.
    eprintln!("\nwindow               mean PM2.5   mean AQI (US)   hours");
    for (label, from, to) in [
        ("2024 (partial)", "2024-01-01", "2024-12-31"),
        ("2025 full year", "2025-01-01", "2025-12-31"),
        ("2025 Jan-Oct 14", "2025-01-01", "2025-10-14"),
        ("2026 Jan-Oct 14", "2026-01-01", "2026-10-14"),
        ("Oct 1-13 2025", "2025-10-01", "2025-10-13"),
        ("Oct 1-13 2026", "2026-10-01", "2026-10-13"),
    ] {
        let window: Vec<f32> = hours
            .iter()
            .filter(|(time, _)| {
                let day = &time[..10];
                day >= from && day <= to
            })
            .map(|(_, value)| *value)
            .collect();
        if window.is_empty() {
            continue;
        }
        let mean = window.iter().sum::<f32>() / window.len() as f32;
        eprintln!(
            "  {label:<18} {:>6.1}        {:>5.0}          {}",
            mean,
            us_aqi(mean),
            window.len()
        );
    }
    eprintln!(
        "\nNote: CAMS is a 45 km global model archive, not a station reading. Compare \
         shape and trend across windows; never compare its level with a monitor."
    );
}

/// `GET` a URL with native TLS. `ureq` never wires a TLS backend by itself on
/// this build (see `core/src/http.rs`), so a live probe has to hand it a
/// `native-tls` connector, the same way every real source does.
fn probe_get(url: &str) -> Result<String, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| format!("TLS init failed: {e}"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(tls))
        .build();
    agent
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())
}

/// US AQI for an hourly PM2.5 value, using the 2024 breakpoints - the same
/// conversion IQAir applies to an instantaneous reading.
fn us_aqi(pm2_5: f32) -> f32 {
    const BANDS: [(f32, f32, f32, f32); 7] = [
        (0.0, 9.0, 0.0, 50.0),
        (9.1, 35.4, 51.0, 100.0),
        (35.5, 55.4, 101.0, 150.0),
        (55.5, 125.4, 151.0, 200.0),
        (125.5, 225.4, 201.0, 300.0),
        (225.5, 325.4, 301.0, 500.0),
        (325.5, 500.0, 301.0, 500.0),
    ];
    for (lo, hi, aqi_lo, aqi_hi) in BANDS {
        if pm2_5 <= hi {
            return aqi_lo + (aqi_hi - aqi_lo) * (pm2_5 - lo) / (hi - lo);
        }
    }
    500.0
}
