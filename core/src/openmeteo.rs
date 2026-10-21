//! Real data source: [Open-Meteo Air Quality](https://open-meteo.com/en/docs/air-quality-api)
//! (no API key, CC BY 4.0). This is the only module in `core` that performs I/O.
//!
//! Design notes:
//! - The HTTP call is **blocking**; the app runs `fetch` off the UI thread and
//!   the trait stays synchronous (no async runtime in `core`).
//! - Response parsing is a **pure function** (`parse_openmeteo`) so it can be
//!   unit-tested against the committed fixture with no network.
//! - We ask for `timeformat=unixtime` (epoch seconds) and attach the offset the
//!   API reports in `utc_offset_seconds`. That is correct across Egypt's DST
//!   changes without pulling in a timezone database.

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::location::Location;
use crate::model::{AirQualityPoint, Series};
use crate::source::{AirQualitySource, SourceError};

const DEFAULT_ENDPOINT: &str = "https://air-quality-api.open-meteo.com";
/// 24 h observed + ~48 h ahead, matching the widget's horizon.
const PAST_DAYS: u8 = 1;
const FORECAST_DAYS: u8 = 3;
/// Fail fast instead of hanging the background fetch.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Open-Meteo Air Quality client for one location.
pub struct OpenMeteoSource {
    latitude: f64,
    longitude: f64,
    timezone: String,
    /// Display label carried into the [`Series`] (shown in the header).
    label: String,
    /// Base endpoint, without the path; overridable so a test can force an error.
    endpoint: String,
    agent: ureq::Agent,
}

impl OpenMeteoSource {
    /// The app's configured source: Cairo, the real Open-Meteo endpoint.
    pub fn cairo() -> Result<Self, SourceError> {
        Self::for_location(&Location::cairo())
    }

    /// A source for `location` against the default Open-Meteo endpoint.
    pub fn for_location(location: &Location) -> Result<Self, SourceError> {
        Self::from_location(location, DEFAULT_ENDPOINT)
    }

    /// A source for a concrete [`Location`]s.
    pub fn from_location(location: &Location, endpoint: &str) -> Result<Self, SourceError> {
        // ureq's `native-tls` feature only provides the `TlsConnector` impl; it
        // never wires a backend by default, so we build and hand one in.
        let tls = native_tls::TlsConnector::new()
            .map_err(|e| SourceError::Network(format!("native TLS init failed: {e}")))?;
        let agent = ureq::AgentBuilder::new()
            .timeout(TIMEOUT)
            .tls_connector(Arc::new(tls))
            .build();
        Ok(Self {
            latitude: location.lat,
            longitude: location.lon,
            timezone: location.tz.clone(),
            label: location.label.clone(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            agent,
        })
    }

    /// The exact request URL (kept public so it can be logged/inspected).
    pub fn url(&self) -> String {
        format!(
            "{endpoint}/v1/air-quality?latitude={lat}&longitude={lon}\
             &hourly=pm2_5,pm10,us_aqi&timezone={tz}&timeformat=unixtime\
             &past_days={past}&forecast_days={fc}",
            endpoint = self.endpoint,
            lat = self.latitude,
            lon = self.longitude,
            tz = self.timezone,
            past = PAST_DAYS,
            fc = FORECAST_DAYS,
        )
    }
}

impl AirQualitySource for OpenMeteoSource {
    fn fetch(&self) -> Result<Series, SourceError> {
        let body = self
            .agent
            .get(&self.url())
            .call()
            .map_err(|e| SourceError::Network(e.to_string()))?
            .into_string()
            .map_err(|e| SourceError::Network(format!("reading response body: {e}")))?;
        parse_openmeteo(&body, Utc::now(), &self.label)
    }
}

/// Wire shape of the bits of the Open-Meteo response we use.
#[derive(Deserialize)]
struct Response {
    utc_offset_seconds: i32,
    hourly: Hourly,
}

#[derive(Deserialize)]
struct Hourly {
    time: Vec<i64>,
    pm2_5: Vec<Option<f32>>,
    #[serde(default)]
    pm10: Vec<Option<f32>>,
    #[serde(default)]
    us_aqi: Vec<Option<f32>>,
}

/// Turn an Open-Meteo JSON body into a [`Series`], with `now_index` set to the
/// hour containing `now` (UTC). Pure: the clock is injected, so tests are
/// deterministic. `location` is the display label carried into the series.
///
/// Hours whose `pm2_5` is null are skipped (the rule needs PM2.5); the API
/// returns nulls at some series edges.
pub fn parse_openmeteo(
    json: &str,
    now: DateTime<Utc>,
    location: &str,
) -> Result<Series, SourceError> {
    let response: Response =
        serde_json::from_str(json).map_err(|e| SourceError::Parse(e.to_string()))?;

    let offset = FixedOffset::east_opt(response.utc_offset_seconds).ok_or_else(|| {
        SourceError::Parse(format!(
            "invalid utc_offset_seconds: {}",
            response.utc_offset_seconds
        ))
    })?;

    let hourly = response.hourly;
    if hourly.time.len() != hourly.pm2_5.len() {
        return Err(SourceError::Parse(format!(
            "hourly time/pm2_5 length mismatch: {} vs {}",
            hourly.time.len(),
            hourly.pm2_5.len()
        )));
    }

    let mut points = Vec::new();
    for (i, &epoch) in hourly.time.iter().enumerate() {
        let Some(pm2_5) = hourly.pm2_5[i] else { continue };
        let time = Utc
            .timestamp_opt(epoch, 0)
            .single()
            .ok_or_else(|| SourceError::Parse(format!("invalid epoch second: {epoch}")))?
            .with_timezone(&offset);
        points.push(AirQualityPoint {
            time,
            pm2_5,
            // Open-Meteo's AQ API reports no wind; the field stays 0.0 for now
            // (the verdict does not use wind).
            wind_speed: 0.0,
            pm10: hourly.pm10.get(i).copied().flatten().unwrap_or(0.0),
            us_aqi: hourly.us_aqi.get(i).copied().flatten().unwrap_or(0.0),
        });
    }

    if points.is_empty() {
        return Err(SourceError::Parse("no hourly points with pm2_5".into()));
    }

    let now_local = now.with_timezone(&offset);
    let now_index = derive_now_index(&points, now_local);
    Ok(Series::new(location, points, now_index))
}

/// Index of the hour containing `now`: the last point not after `now`. Clamps to
/// the first point when `now` precedes the series and to the last when it
/// follows it (points are chronologically ascending).
fn derive_now_index(points: &[AirQualityPoint], now: DateTime<FixedOffset>) -> usize {
    crate::model::now_index_for(points, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture() -> String {
        include_str!("../tests/fixtures/openmeteo-cairo.json").to_string()
    }

    fn cairo_noon() -> DateTime<Utc> {
        // A fixed instant inside the fixture's window (UTC).
        Utc.with_ymd_and_hms(2026, 10, 4, 9, 0, 0).single().unwrap()
    }

    #[test]
    fn parses_fixture_into_a_well_formed_series() {
        let series = parse_openmeteo(&fixture(), cairo_noon(), "Cairo, Egypt").unwrap();
        assert!(!series.points.is_empty());
        assert_eq!(series.location, "Cairo, Egypt");
        // Ascending, one hour apart, all carrying the API's +03:00 offset.
        assert!(
            series
                .points
                .windows(2)
                .all(|w| w[1].time - w[0].time == chrono::Duration::hours(1))
        );
        assert!(series.points.iter().all(|p| p.time.offset().local_minus_utc() == 3 * 3600));
        assert!(series.points.iter().all(|p| p.pm2_5 > 0.0));
    }

    #[test]
    fn now_index_points_at_the_hour_containing_now() {
        let instant = cairo_noon();
        let series = parse_openmeteo(&fixture(), instant, "Cairo, Egypt").unwrap();
        // The series carries +03:00, so the injected 09:00Z is local 12:00.
        let now_local = instant.with_timezone(series.points[0].time.offset());
        assert_eq!(now_local.format("%H:%M").to_string(), "12:00");
        assert!(series.points[series.now_index].time <= now_local);
        if series.now_index + 1 < series.points.len() {
            assert!(series.points[series.now_index + 1].time > now_local);
        }
    }

    #[test]
    fn now_before_and_after_the_series_clamp() {
        let before = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).single().unwrap();
        let after = Utc.with_ymd_and_hms(2100, 1, 1, 0, 0, 0).single().unwrap();
        assert_eq!(parse_openmeteo(&fixture(), before, "Cairo, Egypt").unwrap().now_index, 0);
        let s = parse_openmeteo(&fixture(), after, "Cairo, Egypt").unwrap();
        assert_eq!(s.now_index, s.points.len() - 1);
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let err = parse_openmeteo("{ not json", cairo_noon(), "Cairo, Egypt").unwrap_err();
        assert!(matches!(err, SourceError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn missing_pm2_5_hours_are_skipped() {
        let json = r#"{
            "utc_offset_seconds": 10800,
            "hourly": {
                "time": [1791133200, 1791136800, 1791140400],
                "pm2_5": [null, 12.5, null],
                "pm10": [1.0, 2.0, 3.0],
                "us_aqi": [10, 20, 30]
            }
        }"#;
        let series = parse_openmeteo(json, cairo_noon(), "Cairo, Egypt").unwrap();
        assert_eq!(series.points.len(), 1);
        assert_eq!(series.points[0].pm2_5, 12.5);
        assert_eq!(series.now_index, 0);
    }

    #[test]
    fn url_carries_the_selected_location() {
        let tokyo = crate::location::Location::from_city(
            crate::geo::city_by_id(1850147).expect("Tokyo in the baked set"),
        );
        assert_eq!(tokyo.label, "Tokyo, Japan");
        let source = OpenMeteoSource::from_location(&tokyo, "https://example.test/").unwrap();
        let url = source.url();
        assert!(url.starts_with("https://example.test/v1/air-quality?"), "{url}");
        assert!(url.contains(&format!("latitude={}", tokyo.lat)), "{url}");
        assert!(url.contains(&format!("longitude={}", tokyo.lon)), "{url}");
        assert!(url.contains(&format!("timezone={}", tokyo.tz)), "{url}");
    }
}
