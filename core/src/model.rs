//! The data we reason about, independent of where it came from.

use chrono::{DateTime, FixedOffset};

/// One hour of outdoor air quality at the app's location.
#[derive(Debug, Clone, PartialEq)]
pub struct AirQualityPoint {
    /// Wall-clock time at the location (offset-carrying, so no timezone math later).
    pub time: DateTime<FixedOffset>,
    /// Fine particulate matter, µg/m³ — the value the rule actually uses.
    pub pm2_5: f32,
    /// Coarse particulate matter, µg/m³ — shown/compared, not yet in the rule.
    pub pm10: f32,
    /// US AQI for context (EXP01: nearly flat for Cairo, so never the headline).
    pub us_aqi: f32,
    /// Wind speed in km/h — carried in the model, not yet driving the verdict.
    pub wind_speed: f32,
}

/// A chronological hourly series split at "now" into observed past + forecast.
#[derive(Debug, Clone)]
pub struct Series {
    pub location: String,
    pub points: Vec<AirQualityPoint>,
    /// Index of the point that represents the present moment.
    pub now_index: usize,
}

impl Series {
    pub fn new(location: impl Into<String>, points: Vec<AirQualityPoint>, now_index: usize) -> Self {
        assert!(!points.is_empty(), "series needs at least one point");
        assert!(now_index < points.len(), "now_index out of range");
        Self {
            location: location.into(),
            points,
            now_index,
        }
    }

    /// The present hour.
    pub fn now(&self) -> &AirQualityPoint {
        &self.points[self.now_index]
    }

    /// Advance "now" by one hour; at the end of the window, wrap to the start.
    ///
    /// The app's 3 s ticker calls this so the post moves as time passes. The
    /// `last -> 0` wrap is the end-of-timeline / 11:59:59 PM case: the hour after
    /// the last one is the first one.
    pub fn advance_now(&mut self) {
        self.now_index = if self.now_index + 1 >= self.points.len() {
            0
        } else {
            self.now_index + 1
        };
    }

    /// Observed points, ending at "now" (inclusive).
    pub fn past(&self) -> &[AirQualityPoint] {
        &self.points[..=self.now_index]
    }

    /// Forecast points, starting at "now" (inclusive) so the curve is continuous.
    pub fn forecast(&self) -> &[AirQualityPoint] {
        &self.points[self.now_index..]
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Index of the hour containing `now`: the last point not after `now`. Clamps to
/// the first point when `now` precedes the series and to the last when it
/// follows it. Points must be chronologically ascending.
pub fn now_index_for(points: &[AirQualityPoint], now: DateTime<FixedOffset>) -> usize {
    points
        .partition_point(|p| p.time <= now)
        .saturating_sub(1)
        .min(points.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn series_of(n: usize, now_index: usize) -> Series {
        let tz = FixedOffset::east_opt(0).unwrap();
        let start = tz.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().unwrap();
        let points = (0..n)
            .map(|i| AirQualityPoint {
                time: start + chrono::Duration::hours(i as i64),
                pm2_5: 10.0,
                pm10: 0.0,
                us_aqi: 0.0,
                wind_speed: 0.0,
            })
            .collect();
        Series::new("test", points, now_index)
    }

    #[test]
    fn advance_now_moves_one_hour() {
        let mut series = series_of(48, 23);
        series.advance_now();
        assert_eq!(series.now_index, 24);
    }

    #[test]
    fn advance_now_wraps_from_the_end_to_the_beginning() {
        let mut series = series_of(48, 47);
        series.advance_now();
        assert_eq!(series.now_index, 0, "the hour after the last is the first");
    }

    #[test]
    fn advance_now_walks_every_hour_then_restarts() {
        let n = 48;
        let mut series = series_of(n, 23);
        let start = series.now_index;

        // Tick a full cycle, recording where the post is after each tick.
        let mut visited = vec![start];
        for _ in 0..n {
            series.advance_now();
            visited.push(series.now_index);
        }

        // One cycle of n ticks visits every hour exactly once.
        let mut cycle = visited[..n].to_vec();
        cycle.sort_unstable();
        assert_eq!(
            cycle,
            (0..n).collect::<Vec<_>>(),
            "a full cycle must visit every hour once"
        );

        // It really reaches the last hour, then restarts at the beginning.
        let end = visited
            .iter()
            .position(|&i| i == n - 1)
            .expect("the post must reach the end");
        assert_eq!(visited[end + 1], 0, "the hour after the end is the beginning");

        // The full cycle returns to where it started.
        assert_eq!(series.now_index, start);
    }
}
