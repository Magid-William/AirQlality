//! The data-source boundary plus the mock provider used until Open-Meteo lands.

use chrono::{Duration, FixedOffset, TimeZone};

use crate::model::{AirQualityPoint, Series};

#[derive(Debug)]
pub enum SourceError {
    Unavailable(String),
    /// The request could not be made or did not return a success status
    /// (DNS, TLS, timeout, HTTP 4xx/5xx).
    Network(String),
    /// The response arrived but could not be understood.
    Parse(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::Unavailable(msg) => write!(f, "source unavailable: {msg}"),
            SourceError::Network(msg) => write!(f, "network error: {msg}"),
            SourceError::Parse(msg) => write!(f, "could not parse response: {msg}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// Anything that can produce a [`Series`] for the configured location.
///
/// The mock provider and a future Open-Meteo client implement this, so the app
/// never learns where its data came from.
pub trait AirQualitySource {
    fn fetch(&self) -> Result<Series, SourceError>;
}

/// Deterministic 48 h mock shaped from EXP01's live Cairo probe
/// (`prior-art.md` §5: PM2.5 14–26 µg/m³). 24 h observed + 24 h forecast, with
/// "now" pinned to 2026-10-04 14:00 +02:00 so screenshots are reproducible.
pub struct MockSource;

impl MockSource {
    /// Hourly PM2.5 (µg/m³). Index 23 is "now"; a clean run opens at 35–39.
    const PM25: [f32; 48] = [
        // observed (0..=23), ending at "now"
        26.0, 25.0, 24.0, 23.0, 22.0, 21.0, 20.0, 19.0, 18.0, 17.5, 17.0, 16.5, 16.0, 16.5, 17.0,
        18.0, 19.0, 20.5, 22.0, 24.0, 25.5, 26.5, 27.0, 26.0, // forecast (24..47)
        25.0, 24.0, 23.0, 22.0, 21.0, 20.0, 19.0, 18.0, 17.0, 16.0, 15.5, 15.0, 14.5, 14.0, 14.0,
        14.5, 15.5, 17.0, 19.0, 21.0, 23.0, 24.5, 25.5, 26.0,
    ];
    const NOW_INDEX: usize = 23;
}

impl AirQualitySource for MockSource {
    fn fetch(&self) -> Result<Series, SourceError> {
        let tz = FixedOffset::east_opt(2 * 3600).expect("+02:00 is valid");
        let now = tz
            .with_ymd_and_hms(2026, 10, 4, 14, 0, 0)
            .single()
            .expect("mock instant is valid");

        let points = Self::PM25
            .iter()
            .enumerate()
            .map(|(i, &pm2_5)| {
                let offset = i as i64 - Self::NOW_INDEX as i64;
                AirQualityPoint {
                    time: now + Duration::hours(offset),
                    pm2_5,
                    pm10: pm2_5 * 2.1 + 8.0,
                    us_aqi: (pm2_5 * 2.9).round(),
                    wind_speed: 9.0 + (i % 7) as f32 * 1.3,
                }
            })
            .collect();

        Ok(Series::new(
            crate::location::Location::cairo().label,
            points,
            Self::NOW_INDEX,
        ))
    }
}

/// EXP19 test seam: three deterministic series with a controlled spread, so the
/// stacking UI can be exercised with no network and no keys.
///
/// The first is the plain mock (so a single-source run stays byte-identical),
/// then an observation-like station that runs 8 µg/m³ higher and a second
/// model that runs 10 % lower. Around "now" (26 µg/m³) that puts the ground
/// station in KEEP CLOSED and the second model in VENT, i.e. the sources
/// straddle a threshold on purpose.
pub fn mock_stack() -> Vec<crate::fuse::SourceSeries> {
    use crate::fuse::{Kind, SourceSeries};

    let base = MockSource.fetch().expect("mock source is infallible");
    let shaped = |scale: f32, offset: f32| -> Series {
        let points = base
            .points
            .iter()
            .map(|p| AirQualityPoint {
                time: p.time,
                pm2_5: (p.pm2_5 * scale + offset).max(0.0),
                pm10: 0.0,
                us_aqi: 0.0,
                wind_speed: 0.0,
            })
            .collect();
        Series::new(base.location.clone(), points, base.now_index)
    };

    vec![
        SourceSeries {
            id: "open-meteo",
            label: "Open-Meteo",
            kind: Kind::Model,
            series: base.clone(),
        },
        SourceSeries {
            id: "openaq",
            label: "OpenAQ",
            kind: Kind::Ground,
            series: shaped(1.0, 8.0),
        },
        SourceSeries {
            id: "openweathermap",
            label: "OpenWeatherMap",
            kind: Kind::Model,
            series: shaped(0.9, 0.0),
        },
    ]
}
