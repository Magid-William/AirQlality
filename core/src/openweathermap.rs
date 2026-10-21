//! Second model source: [OpenWeatherMap Air Pollution](https://openweathermap.org/api/air-pollution)
//! (free API key, hourly forecast for 4 days).
//!
//! This is a **second model**, kept for redundancy over the whole timeline: it
//! forecasts the future, so the stack does not collapse to a single model above
//! "now". Its lineage is CAMS-based like Open-Meteo's, so agreement between the
//! two is *not* independent evidence — that is what the OpenAQ ground station is
//! for, and the UI says which is which.
//!
//! The response timebase is plain epoch seconds with no offset, so points are
//! stamped UTC; the fusion layer compares sources by instant and re-stamps the
//! result on the reference (Open-Meteo) wall clock.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::http;
use crate::location::Location;
use crate::model::{AirQualityPoint, Series};
use crate::source::{AirQualitySource, SourceError};

const DEFAULT_ENDPOINT: &str = "https://api.openweathermap.org";
/// The hourly forecast endpoint covers 4 days from "now".
const TIMEOUT: Duration = Duration::from_secs(20);

/// OpenWeatherMap Air Pollution client for one location.
pub struct OpenWeatherMapSource {
    latitude: f64,
    longitude: f64,
    label: String,
    endpoint: String,
    key: String,
    agent: ureq::Agent,
}

impl OpenWeatherMapSource {
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

    /// Check the key with one cheap request, without parsing a series (EXP24).
    /// `Ok(())` means the provider accepted the credential; a 401/403 comes back
    /// as [`SourceError::Rejected`].
    pub fn check_key(&self) -> Result<(), SourceError> {
        http::get_redacted(&self.agent, &self.url(), &[], &[self.key.as_str()]).map(|_| ())
    }

    /// The forecast URL (public so it can be logged/inspected). The key is a
    /// query parameter, as the API requires.
    pub fn url(&self) -> String {
        format!(
            "{endpoint}/data/2.5/air_pollution/forecast?lat={lat}&lon={lon}&appid={key}",
            endpoint = self.endpoint,
            lat = self.latitude,
            lon = self.longitude,
            key = self.key,
        )
    }
}

impl AirQualitySource for OpenWeatherMapSource {
    fn fetch(&self) -> Result<Series, SourceError> {
        // The key is a query parameter, so scrub it out of any error (ureq puts
        // the URL in transport errors).
        let body = http::get_redacted(&self.agent, &self.url(), &[], &[self.key.as_str()])?;
        parse_openweathermap(&body, Utc::now(), &self.label)
    }
}

/// Wire shape of `/data/2.5/air_pollution/forecast`.
#[derive(Deserialize)]
struct Response {
    list: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    dt: i64,
    #[serde(default)]
    components: Components,
}

#[derive(Deserialize, Default)]
struct Components {
    #[serde(default)]
    pm2_5: Option<f32>,
    #[serde(default)]
    pm10: Option<f32>,
}

/// Turn an OpenWeatherMap body into a [`Series`] (UTC timestamps). Pure: the
/// clock is injected. Entries without `pm2_5` are skipped.
pub fn parse_openweathermap(
    json: &str,
    now: DateTime<Utc>,
    location: &str,
) -> Result<Series, SourceError> {
    let response: Response =
        serde_json::from_str(json).map_err(|e| SourceError::Parse(e.to_string()))?;

    let mut points: Vec<AirQualityPoint> = response
        .list
        .iter()
        .filter_map(|entry| {
            let pm2_5 = entry.components.pm2_5?;
            let time = DateTime::from_timestamp(entry.dt, 0)?.fixed_offset();
            Some(AirQualityPoint {
                time,
                pm2_5,
                pm10: entry.components.pm10.unwrap_or(0.0),
                us_aqi: 0.0,
                wind_speed: 0.0,
            })
        })
        .collect();

    if points.is_empty() {
        return Err(SourceError::Parse("no hourly PM2.5 values".into()));
    }
    points.sort_by_key(|p| p.time);

    let now_index = crate::model::now_index_for(&points, now.fixed_offset());
    Ok(Series::new(location, points, now_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture() -> String {
        include_str!("../tests/fixtures/openweathermap-cairo.json").to_string()
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).single().unwrap()
    }

    #[test]
    fn parses_the_fixture_into_an_hourly_series() {
        let series = parse_openweathermap(&fixture(), now(), "Cairo, Egypt").unwrap();
        assert!(!series.points.is_empty());
        assert!(series.points.windows(2).all(|w| w[1].time > w[0].time));
        assert!(series.points.iter().all(|p| p.pm2_5 > 0.0));
        assert!(series.now_index < series.points.len());
    }

    #[test]
    fn now_index_lands_on_the_hour_containing_now() {
        let series = parse_openweathermap(&fixture(), now(), "Cairo, Egypt").unwrap();
        let stamped = series.points[series.now_index].time;
        assert!(stamped <= now().fixed_offset());
        assert!(series.points[series.now_index + 1].time > now().fixed_offset());
    }

    #[test]
    fn entries_without_pm2_5_are_skipped() {
        let json = r#"{"list":[
            {"dt":1792080000,"components":{"pm2_5":12.0,"pm10":20.0}},
            {"dt":1792083600,"components":{"pm10":30.0}},
            {"dt":1792087200,"components":{"pm2_5":13.0}}
        ]}"#;
        let series = parse_openweathermap(json, now(), "x").unwrap();
        assert_eq!(series.points.len(), 2);
    }

    #[test]
    fn a_body_without_any_pm2_5_is_an_error() {
        let json = r#"{"list":[{"dt":1792080000,"components":{"pm10":1.0}}]}"#;
        assert!(matches!(
            parse_openweathermap(json, now(), "x").unwrap_err(),
            SourceError::Parse(_)
        ));
    }

    #[test]
    fn url_carries_the_location_and_the_key() {
        let cairo = Location::cairo();
        let source = OpenWeatherMapSource::from_location(&cairo, "k", "https://example.test/")
            .unwrap();
        let url = source.url();
        assert!(url.starts_with("https://example.test/data/2.5/air_pollution/forecast?"));
        assert!(url.contains(&format!("lat={}", cairo.lat)));
        assert!(url.contains("appid=k"));
    }
}
