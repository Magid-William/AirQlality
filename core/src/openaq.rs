//! Ground-truth source: [OpenAQ](https://openaq.org) v3 (free API key).
//!
//! OpenAQ aggregates *physical* monitoring stations — government networks and
//! embassy monitors — which is the axis the CAMS model front-ends (Open-Meteo,
//! OpenWeatherMap) cannot provide. It measures the past and "now" only; it has
//! no forecast.
//!
//! Two requests, both pure-parsed so they can be tested against committed
//! fixtures with no network:
//!
//! 1. `/v3/locations?coordinates=…&radius=…&parameters_id=2` -> nearest fresh
//!    PM2.5 station ([`nearest_pm25_sensor`]).
//! 2. `/v3/sensors/{id}/hours` -> hourly means ([`parse_openaq_hours`]).
//!
//! `datetime.local` carries its own offset (e.g. `+03:00`), so the series needs
//! no external timezone input — important because OpenAQ, unlike Open-Meteo,
//! does not report `utc_offset_seconds`.

use std::time::Duration;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;

use crate::http;
use crate::location::Location;
use crate::model::{AirQualityPoint, Series};
use crate::source::{AirQualitySource, SourceError};

const DEFAULT_ENDPOINT: &str = "https://api.openaq.org";
/// Half-width of the search box around the location, in degrees (~66 km at
/// Cairo's latitude). OpenAQ caps `radius` at **25 km**, so a box is the only
/// way to see the nearest station and say how far away it is.
const BOX_DEG: f64 = 0.6;
/// The furthest a ground station may sit from the requested location and still
/// be used: a station further away does not describe this city's air, and
/// presenting it as "ground truth" would be a lie rather than a stack.
pub const MAX_KM: f64 = 25.0;
/// Hours of history to request; the window ends at "now".
const HOURS_BACK: i64 = 48;
/// A station counts as "fresh" when it reported within this many hours.
const FRESH_HOURS: i64 = 12;
const TIMEOUT: Duration = Duration::from_secs(20);

/// OpenAQ v3 client for one location.
pub struct OpenAqSource {
    latitude: f64,
    longitude: f64,
    label: String,
    endpoint: String,
    key: String,
    agent: ureq::Agent,
}

impl OpenAqSource {
    /// A source for `location` against the default endpoint, using `key`.
    pub fn for_location(location: &Location, key: &str) -> Result<Self, SourceError> {
        Self::from_location(location, key, DEFAULT_ENDPOINT)
    }

    pub fn from_location(
        location: &Location,
        key: &str,
        endpoint: &str,
    ) -> Result<Self, SourceError> {
        Ok(Self {
            latitude: location.lat,
            longitude: location.lon,
            label: location.label.clone(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            key: key.to_string(),
            agent: http::agent(TIMEOUT)?,
        })
    }

    /// The nearest-station lookup URL (public so it can be logged/inspected).
    pub fn stations_url(&self) -> String {
        let (min_lon, min_lat) = (self.longitude - BOX_DEG, self.latitude - BOX_DEG);
        let (max_lon, max_lat) = (self.longitude + BOX_DEG, self.latitude + BOX_DEG);
        format!(
            "{endpoint}/v3/locations?bbox={min_lon},{min_lat},{max_lon},{max_lat}\
             &parameters_id=2&limit=100",
            endpoint = self.endpoint,
        )
    }

    /// The hourly-series URL for a station, windowed to end at "now": the
    /// endpoint returns the station's *oldest* hours by default, so a station
    /// that stopped reporting years ago would otherwise look current.
    pub fn hours_url(&self, sensor_id: u64, from: DateTime<Utc>) -> String {
        format!(
            "{endpoint}/v3/sensors/{sensor_id}/hours?datetime_from={from}&limit=48",
            endpoint = self.endpoint,
            from = from.to_rfc3339_opts(SecondsFormat::Secs, true),
        )
    }
}

impl AirQualitySource for OpenAqSource {
    fn fetch(&self) -> Result<Series, SourceError> {
        let auth = [("X-API-Key", self.key.as_str())];
        // The key travels in a header, but scrub it from errors anyway.
        let secret = [self.key.as_str()];

        let locations = http::get_redacted(&self.agent, &self.stations_url(), &auth, &secret)?;
        let now = Utc::now();
        let station = nearest_pm25_sensor(&locations, self.latitude, self.longitude, now)?;

        let from = now - chrono::Duration::hours(HOURS_BACK);
        let hours =
            http::get_redacted(&self.agent, &self.hours_url(station.id, from), &auth, &secret)?;
        parse_openaq_hours(&hours, now, &self.label)
    }
}

/// The station chosen for a location.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    /// The PM2.5 sensor to read.
    pub id: u64,
    /// Station name, for honest error messages ("Walkthru Earth Main").
    pub name: String,
    /// Straight-line distance from the requested location, km.
    pub distance_km: f64,
    /// When the station last reported, when the API said so.
    pub last_seen: Option<DateTime<Utc>>,
}

/// Wire shape of `/v3/locations` (only the fields we use).
#[derive(Deserialize)]
struct LocationsResponse {
    results: Vec<WireLocation>,
}

#[derive(Deserialize)]
struct WireLocation {
    coordinates: Coordinates,
    #[serde(default)]
    sensors: Vec<Sensor>,
    #[serde(default, rename = "datetimeLast")]
    datetime_last: Option<Datetime>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct Coordinates {
    latitude: f64,
    longitude: f64,
}

#[derive(Deserialize)]
struct Sensor {
    id: u64,
    parameter: Parameter,
}

#[derive(Deserialize)]
struct Parameter {
    name: String,
}

/// Wire shape of `/v3/sensors/{id}/hours` (only the fields we use). The API's
/// hourly record carries its timestamp under `period.datetimeFrom`; some other
/// endpoints put a `datetime` on the record, so both are accepted.
#[derive(Deserialize)]
struct HoursResponse {
    results: Vec<Hour>,
}

#[derive(Deserialize)]
struct Hour {
    value: Option<f32>,
    #[serde(default)]
    parameter: Option<Parameter>,
    #[serde(default)]
    datetime: Option<Datetime>,
    #[serde(default)]
    period: Option<Period>,
}

#[derive(Deserialize)]
struct Period {
    #[serde(rename = "datetimeFrom")]
    datetime_from: Datetime,
}

#[derive(Deserialize)]
struct Datetime {
    utc: String,
    /// OpenAQ reports the station's local wall clock with its offset; when it is
    /// present we prefer it, so the series carries `+03:00` for Cairo.
    #[serde(default)]
    local: Option<String>,
}

/// OpenAQ v3 calls the parameter `pm25`; older payloads used `pm2_5`.
fn is_pm25(name: &str) -> bool {
    name == "pm25" || name == "pm2_5"
}

/// Pick the PM2.5 sensor to read: the **nearest fresh** station, if it is close
/// enough to describe this location (see [`MAX_KM`]). Pure (the clock is
/// injected). Every rejection explains itself, because "no ground station" and
/// "the ground station is 38 km away" mean very different things about the
/// stack. Messages stay short: they are shown in the widget's stale note.
pub fn nearest_pm25_sensor(
    json: &str,
    lat: f64,
    lon: f64,
    now: DateTime<Utc>,
) -> Result<Station, SourceError> {
    let response: LocationsResponse =
        serde_json::from_str(json).map_err(|e| SourceError::Parse(e.to_string()))?;

    let candidates: Vec<Station> = response
        .results
        .iter()
        .filter_map(|loc| {
            let sensor = loc.sensors.iter().find(|s| is_pm25(&s.parameter.name))?;
            Some(Station {
                id: sensor.id,
                name: loc.name.clone().unwrap_or_else(|| "unnamed".to_string()),
                distance_km: haversine_km(
                    lat,
                    lon,
                    loc.coordinates.latitude,
                    loc.coordinates.longitude,
                ),
                last_seen: loc
                    .datetime_last
                    .as_ref()
                    .and_then(|d| DateTime::parse_from_rfc3339(&d.utc).ok())
                    .map(|t| t.with_timezone(&Utc)),
            })
        })
        .collect();

    if candidates.is_empty() {
        return Err(SourceError::Unavailable(format!(
            "no PM2.5 station within {} km",
            box_km()
        )));
    }

    let is_fresh = |s: &Station| {
        s.last_seen
            .map(|t| now.signed_duration_since(t) <= chrono::Duration::hours(FRESH_HOURS))
            .unwrap_or(false)
    };
    let nearest = |list: &mut dyn Iterator<Item = &Station>| {
        list.min_by(|a, b| {
            a.distance_km
                .partial_cmp(&b.distance_km)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
    };

    let fresh = nearest(&mut candidates.iter().filter(|s| is_fresh(s)));
    match fresh {
        Some(station) if station.distance_km <= MAX_KM => Ok(station),
        Some(station) => Err(SourceError::Unavailable(format!(
            "no fresh PM2.5 station within {:.0} km (nearest: {} at {:.0} km)",
            MAX_KM, station.name, station.distance_km
        ))),
        None => {
            let stale = nearest(&mut candidates.iter());
            let when = stale
                .as_ref()
                .and_then(|s| s.last_seen)
                .map(|t| t.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "never".to_string());
            let name = stale.map(|s| s.name).unwrap_or_else(|| "unknown".to_string());
            Err(SourceError::Unavailable(format!(
                "no fresh PM2.5 station (nearest: {name}, last reported {when})"
            )))
        }
    }
}

/// The search box's half-width in kilometres, for the "nothing nearby" message.
fn box_km() -> f64 {
    (BOX_DEG * 111.0).round()
}

/// Turn a `/v3/sensors/{id}/hours` body into a [`Series`]. Pure: the clock is
/// injected. Hours whose value is null are skipped, and the points are sorted
/// ascending.
pub fn parse_openaq_hours(
    json: &str,
    now: DateTime<Utc>,
    location: &str,
) -> Result<Series, SourceError> {
    let response: HoursResponse =
        serde_json::from_str(json).map_err(|e| SourceError::Parse(e.to_string()))?;

    let mut points: Vec<AirQualityPoint> = Vec::new();
    for hour in &response.results {
        let Some(value) = hour.value else { continue };
        // A single sensor reports one parameter, but filter defensively.
        if let Some(p) = &hour.parameter {
            if !is_pm25(&p.name) {
                continue;
            }
        }
        let Some(when) = hour
            .datetime
            .as_ref()
            .or_else(|| hour.period.as_ref().map(|p| &p.datetime_from))
        else {
            continue;
        };
        let raw = when.local.as_deref().unwrap_or(&when.utc);
        let time = DateTime::parse_from_rfc3339(raw)
            .map_err(|e| SourceError::Parse(format!("bad datetime {raw:?}: {e}")))?;
        points.push(AirQualityPoint {
            time,
            pm2_5: value,
            // A ground station reports mass only; the other fields stay empty.
            pm10: 0.0,
            us_aqi: 0.0,
            wind_speed: 0.0,
        });
    }

    if points.is_empty() {
        return Err(SourceError::Parse(
            "no hourly PM2.5 values in the requested window".into(),
        ));
    }
    points.sort_by_key(|p| p.time);

    let offset = points[0].time.offset().to_owned();
    let now_index = crate::model::now_index_for(&points, now.with_timezone(&offset));
    Ok(Series::new(location, points, now_index))
}

/// Great-circle distance in kilometres.
fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture() -> String {
        include_str!("../tests/fixtures/openaq-cairo-hours.json").to_string()
    }

    fn locations() -> String {
        include_str!("../tests/fixtures/openaq-cairo-locations.json").to_string()
    }

    /// The fixture's "now": the fresh station last reported at 07:00Z.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).single().unwrap()
    }

    #[test]
    fn parses_period_timestamps_into_an_ascending_series() {
        let series = parse_openaq_hours(&fixture(), now(), "Cairo, Egypt").unwrap();
        assert_eq!(series.location, "Cairo, Egypt");
        // Five records, one of which has a null value.
        assert_eq!(series.points.len(), 4);
        assert!(series.points.windows(2).all(|w| w[1].time > w[0].time));
        // The Egyptian wall clock is +03:00 and came from the payload itself.
        assert!(series
            .points
            .iter()
            .all(|p| p.time.offset().local_minus_utc() == 3 * 3600));
        assert_eq!(series.points[0].pm2_5, 53.0);
    }

    #[test]
    fn now_index_points_at_the_hour_containing_now() {
        let series = parse_openaq_hours(&fixture(), now(), "Cairo, Egypt").unwrap();
        // The last record starts 08:00Z, i.e. before the injected 12:00Z.
        assert_eq!(series.now_index, series.points.len() - 1);
    }

    #[test]
    fn skips_null_values_and_records_without_a_timestamp() {
        let json = r#"{"results":[
            {"value":12.5,"parameter":{"name":"pm25"},
             "period":{"datetimeFrom":{"utc":"2026-10-15T09:00:00Z"}}},
            {"value":null,"parameter":{"name":"pm25"},
             "period":{"datetimeFrom":{"utc":"2026-10-15T10:00:00Z"}}},
            {"value":13.5,"parameter":{"name":"pm25"}}
        ]}"#;
        let series = parse_openaq_hours(json, now(), "Cairo, Egypt").unwrap();
        assert_eq!(series.points.len(), 1);
    }

    #[test]
    fn accepts_a_bare_datetime_and_the_legacy_pm2_5_name() {
        let json = r#"{"results":[
            {"value":1.0,"datetime":{"utc":"2026-10-15T09:00:00Z"},"parameter":{"name":"pm2_5"}}
        ]}"#;
        let series = parse_openaq_hours(json, now(), "x").unwrap();
        assert_eq!(series.points.len(), 1);
    }

    #[test]
    fn rejects_other_parameters_and_empty_payloads() {
        let other = r#"{"results":[
            {"value":1.0,"parameter":{"name":"pm10"},
             "period":{"datetimeFrom":{"utc":"2026-10-15T09:00:00Z"}}}
        ]}"#;
        assert!(matches!(
            parse_openaq_hours(other, now(), "x").unwrap_err(),
            SourceError::Parse(_)
        ));
        let empty = r#"{"results":[]}"#;
        assert!(parse_openaq_hours(empty, now(), "x").is_err());
    }

    /// The real Cairo situation (2026-10-15): the reference monitor stopped in
    /// March 2025, and the nearest fresh station is a citizen sensor ~38 km out.
    /// The source must refuse rather than pass it off as Cairo's ground truth.
    #[test]
    fn cairo_has_no_fresh_station_close_enough() {
        let err = nearest_pm25_sensor(&locations(), 30.0444, 31.2357, now()).unwrap_err();
        let SourceError::Unavailable(msg) = err else {
            panic!("expected Unavailable, got {err:?}")
        };
        assert!(msg.contains("no fresh PM2.5 station within 25 km"), "{msg}");
        assert!(msg.contains("Walkthru Earth Main"), "{msg}");
        assert!(msg.contains("at 39 km"), "{msg}");
    }

    #[test]
    fn a_fresh_station_within_the_limit_wins_over_a_stale_closer_one() {
        let json = r#"{"results":[
            {"name":"stale close","coordinates":{"latitude":30.05,"longitude":31.24},
             "sensors":[{"id":1,"parameter":{"name":"pm25"}}],
             "datetimeLast":{"utc":"2025-03-04T12:00:00Z"}},
            {"name":"fresh near","coordinates":{"latitude":30.06,"longitude":31.25},
             "sensors":[{"id":2,"parameter":{"name":"pm25"}}],
             "datetimeLast":{"utc":"2026-10-15T07:00:00Z"}}
        ]}"#;
        let station = nearest_pm25_sensor(json, 30.0444, 31.2357, now()).unwrap();
        assert_eq!(station.id, 2);
        assert_eq!(station.name, "fresh near");
        assert!(station.distance_km < MAX_KM);
    }

    #[test]
    fn a_stale_only_network_reports_when_it_last_ran() {
        let json = r#"{"results":[
            {"name":"Cairo","coordinates":{"latitude":30.04147,"longitude":31.23289},
             "sensors":[{"id":1,"parameter":{"name":"pm25"}}],
             "datetimeLast":{"utc":"2025-03-04T12:00:00Z"}}
        ]}"#;
        let err = nearest_pm25_sensor(json, 30.0444, 31.2357, now()).unwrap_err();
        let SourceError::Unavailable(msg) = err else {
            panic!("expected Unavailable, got {err:?}")
        };
        assert!(msg.contains("no fresh PM2.5 station"), "{msg}");
        assert!(msg.contains("2025-03-04"), "{msg}");
    }

    #[test]
    fn no_pm25_sensor_at_all_is_reported_as_such() {
        let json = r#"{"results":[{"name":"x",
            "coordinates":{"latitude":30.0,"longitude":31.2},
            "sensors":[{"id":9,"parameter":{"name":"no2"}}]}]}"#;
        let err = nearest_pm25_sensor(json, 30.0, 31.2, now()).unwrap_err();
        let SourceError::Unavailable(msg) = err else {
            panic!("expected Unavailable, got {err:?}")
        };
        assert!(msg.contains("no PM2.5 station within"), "{msg}");
    }

    #[test]
    fn urls_box_the_location_and_window_the_hours_at_now() {
        let cairo = Location::cairo();
        let source =
            OpenAqSource::from_location(&cairo, "secret", "https://example.test/").unwrap();
        let stations = source.stations_url();
        assert!(stations.starts_with("https://example.test/v3/locations?bbox="), "{stations}");
        assert!(stations.contains("parameters_id=2"), "{stations}");

        let from = now() - chrono::Duration::hours(48);
        let hours = source.hours_url(42, from);
        assert!(
            hours.starts_with("https://example.test/v3/sensors/42/hours?datetime_from=2026-10-13T12:00:00Z"),
            "{hours}"
        );
        // The key is never part of a URL; it travels in a header.
        assert!(!stations.contains("secret") && !hours.contains("secret"));
    }
}
