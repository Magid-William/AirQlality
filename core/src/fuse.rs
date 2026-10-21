//! Stacking several air-quality sources into one robust series (EXP19).
//!
//! The app fetches every configured source, then [`stack`] aligns them on one
//! grid and reports, per hour, the **median** PM2.5 and the min/max spread
//! across the sources that covered that hour. The median is the whole point:
//! with three sources, one bad or stale reading cannot move the headline.
//!
//! Two honesty rules keep the fusion from inventing confidence:
//!
//! - Sources are compared by **instant** (`chrono` orders `DateTime`s across
//!   offsets), and the fused series adopts the *reference* (forecast) source's
//!   timestamps, so the displayed wall clock is the forecast's one.
//! - When the spread straddles a verdict threshold, the hour is flagged
//!   [`disagree`](Stack::disagree) and the **more protective** verdict wins:
//!   doubt about a health decision resolves to "keep the window closed".

use chrono::{DateTime, Duration, FixedOffset};

use crate::decision::{classify, evaluate_hourly, Decision, Thresholds, Verdict};
use crate::model::{AirQualityPoint, Series};
use crate::source::SourceError;

/// How far a source's reading may sit from a grid hour and still count for it.
/// Hourly data jitters by minutes; half an hour is the natural bucket.
const BUCKET: Duration = Duration::minutes(30);

/// Which lineage a source belongs to, so the UI can be honest about what the
/// stack is actually made of (a model forecast, or a ground measurement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A numerical model (CAMS front-ends): forecasts the future.
    Model,
    /// A physical monitoring station: measures the past and "now" only.
    Ground,
}

/// One source's series plus the labels the UI shows for it.
#[derive(Debug, Clone)]
pub struct SourceSeries {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub series: Series,
}

impl SourceSeries {
    /// The reading nearest to `t`, within the bucket window.
    pub fn value_at(&self, t: DateTime<FixedOffset>) -> Option<f32> {
        self.series
            .points
            .iter()
            .filter(|p| (p.time - t).abs() <= BUCKET)
            .min_by_key(|p| (p.time - t).num_seconds().abs())
            .map(|p| p.pm2_5)
    }
}

/// How tightly the sources agree on one hour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    pub min: f32,
    pub max: f32,
    /// How many sources covered this hour (0 = none, falls back to the grid).
    pub count: u8,
}

impl Spread {
    pub fn width(self) -> f32 {
        self.max - self.min
    }
}

/// The fused series plus everything the UI needs to show how it was built.
#[derive(Debug, Clone)]
pub struct Fusion {
    pub sources: Vec<SourceSeries>,
    /// Median PM2.5 per hour, on the reference source's timestamps.
    pub series: Series,
    /// Per-hour spread, same order as `series.points`.
    pub spread: Vec<Spread>,
}

/// One source's reading for the "now" hour, for the sources strip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceReading {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    /// `None` when the source had no reading within the bucket of "now".
    pub pm2_5: Option<f32>,
}

/// The fused series, its decision, and the per-hour disagreement flags.
#[derive(Debug, Clone)]
pub struct Stack {
    pub fusion: Fusion,
    pub decision: Decision,
    /// `true` where the spread spans more than one verdict band.
    pub disagree: Vec<bool>,
}

impl Stack {
    pub fn series(&self) -> &Series {
        &self.fusion.series
    }

    /// Whether more than one source contributed (i.e. the stack is real).
    pub fn multi(&self) -> bool {
        self.fusion.sources.len() > 1
    }

    /// The spread at "now".
    pub fn now_spread(&self) -> Spread {
        self.fusion.spread[self.fusion.series.now_index]
    }

    /// Whether the sources disagree at "now".
    pub fn now_disagrees(&self) -> bool {
        self.disagree[self.fusion.series.now_index]
    }

    /// Per-source reading for the "now" hour.
    pub fn readings_now(&self) -> Vec<SourceReading> {
        let now = self.fusion.series.now().time;
        self.fusion
            .sources
            .iter()
            .map(|s| SourceReading {
                id: s.id,
                label: s.label,
                kind: s.kind,
                pm2_5: s.value_at(now),
            })
            .collect()
    }
}

/// Fuse `sources` into one [`Stack`] against `thresholds`.
///
/// Requires at least one source. A single source passes through unchanged
/// (median of one), so the app degrades to today's behaviour with no keys.
pub fn stack(sources: Vec<SourceSeries>, thresholds: &Thresholds) -> Result<Stack, SourceError> {
    let fusion = fuse(sources)?;
    let (decision, disagree) = evaluate(&fusion, thresholds);
    Ok(Stack {
        fusion,
        decision,
        disagree,
    })
}

/// Align every source on the reference source's grid and take the per-hour median.
pub fn fuse(sources: Vec<SourceSeries>) -> Result<Fusion, SourceError> {
    let reference = reference_index(&sources).ok_or_else(|| {
        SourceError::Unavailable("no data sources are configured".to_string())
    })?;

    let (grid, now_index) = {
        let s = &sources[reference].series;
        (s.points.clone(), s.now_index)
    };

    let mut points = Vec::with_capacity(grid.len());
    let mut spread = Vec::with_capacity(grid.len());

    for rp in &grid {
        let mut values: Vec<f32> = sources
            .iter()
            .filter_map(|s| s.value_at(rp.time))
            .collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let spread_here = Spread {
            min: values.first().copied().unwrap_or(rp.pm2_5),
            max: values.last().copied().unwrap_or(rp.pm2_5),
            count: values.len().min(u8::MAX as usize) as u8,
        };

        points.push(AirQualityPoint {
            // Keep the reference (forecast) wall clock; other sources only
            // contribute their PM2.5.
            time: rp.time,
            pm2_5: median(&values).unwrap_or(rp.pm2_5),
            pm10: rp.pm10,
            us_aqi: rp.us_aqi,
            wind_speed: rp.wind_speed,
        });
        spread.push(spread_here);
    }

    let location = sources[reference].series.location.clone();
    Ok(Fusion {
        sources,
        series: Series::new(location, points, now_index),
        spread,
    })
}

/// Classify the fused series, applying the conservative tie-break where the
/// sources straddle a band. Pure: thresholds in, verdicts out.
pub fn evaluate(fusion: &Fusion, thresholds: &Thresholds) -> (Decision, Vec<bool>) {
    let n = fusion.fusion_len();
    let mut hourly = Vec::with_capacity(n);
    let mut disagree = Vec::with_capacity(n);

    for i in 0..n {
        let spread = fusion.spread[i];
        let mid = classify(fusion.series.points[i].pm2_5, thresholds);
        let lo = classify(spread.min, thresholds);
        let hi = classify(spread.max, thresholds);
        // More than one band across the spread = the sources straddle a boundary.
        let straddles = spread.count >= 2 && lo != hi;
        let verdict = if straddles {
            [mid, lo, hi]
                .into_iter()
                .max_by_key(severity)
                .expect("three candidates")
        } else {
            mid
        };
        hourly.push(verdict);
        disagree.push(straddles);
    }

    (evaluate_hourly(hourly, fusion.series.now_index), disagree)
}

impl Fusion {
    /// Number of hours on the fused grid (mirrors `series.len()`).
    fn fusion_len(&self) -> usize {
        self.series.points.len()
    }
}

/// How protective a verdict is; the max wins the tie-break.
fn severity(v: &Verdict) -> u8 {
    match v {
        Verdict::Open => 0,
        Verdict::Vent => 1,
        Verdict::KeepClosed => 2,
    }
}

/// Index of the reference source: the model with the most hours wins, so the
/// fused series keeps a forecast horizon and the API's wall-clock offset. Ties
/// keep the *first* source (the app passes Open-Meteo first), which keeps the
/// fused grid deterministic.
fn reference_index(sources: &[SourceSeries]) -> Option<usize> {
    let key = |s: &SourceSeries| (s.kind == Kind::Model, s.series.len(), s.series.now_index);
    let mut best = 0usize;
    let mut best_key = key(sources.first()?);
    for (i, s) in sources.iter().enumerate().skip(1) {
        let k = key(s);
        if k > best_key {
            best = i;
            best_key = k;
        }
    }
    Some(best)
}

/// Median of an ascending slice (even lengths average the middle two).
fn median(sorted: &[f32]) -> Option<f32> {
    match sorted.len() {
        0 => None,
        n if n % 2 == 1 => Some(sorted[n / 2]),
        n => Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn series_with(pm: &[f32], now_index: usize) -> Series {
        let tz = FixedOffset::east_opt(3 * 3600).unwrap();
        let start = tz.with_ymd_and_hms(2026, 10, 15, 0, 0, 0).single().unwrap();
        let points = pm
            .iter()
            .enumerate()
            .map(|(i, &v)| AirQualityPoint {
                time: start + Duration::hours(i as i64),
                pm2_5: v,
                pm10: 0.0,
                us_aqi: 0.0,
                wind_speed: 0.0,
            })
            .collect();
        Series::new("test", points, now_index)
    }

    fn src(id: &'static str, kind: Kind, pm: &[f32]) -> SourceSeries {
        SourceSeries {
            id,
            label: id,
            kind,
            series: series_with(pm, pm.len() / 2),
        }
    }

    #[test]
    fn median_of_three_ignores_a_single_wild_value() {
        let a = src("a", Kind::Model, &[10.0, 20.0, 30.0, 40.0]);
        let b = src("b", Kind::Ground, &[100.0, 20.0, 32.0, 1000.0]);
        let c = src("c", Kind::Model, &[12.0, 24.0, 28.0, 36.0]);
        let fusion = fuse(vec![a, b, c]).unwrap();
        assert_eq!(
            fusion.series.points.iter().map(|p| p.pm2_5).collect::<Vec<_>>(),
            vec![12.0, 20.0, 30.0, 40.0]
        );
    }

    #[test]
    fn spread_reports_min_max_and_count() {
        let a = src("a", Kind::Model, &[10.0, 20.0]);
        let b = src("b", Kind::Ground, &[30.0, 20.0]);
        let fusion = fuse(vec![a, b]).unwrap();
        assert_eq!(fusion.spread[0], Spread { min: 10.0, max: 30.0, count: 2 });
        assert_eq!(fusion.spread[1], Spread { min: 20.0, max: 20.0, count: 2 });
    }

    #[test]
    fn a_source_without_the_hour_does_not_count() {
        // `b` only covers the first two hours, so the later hours are single-source.
        let a = src("a", Kind::Model, &[10.0, 11.0, 12.0, 13.0]);
        let mut b = src("b", Kind::Ground, &[20.0, 21.0]);
        b.series = series_with(&[20.0, 21.0], 1);
        let fusion = fuse(vec![a, b]).unwrap();
        assert_eq!(fusion.spread[3].count, 1);
        assert_eq!(fusion.series.points[3].pm2_5, 13.0);
    }

    #[test]
    fn a_single_source_passes_through_unchanged() {
        let a = src("a", Kind::Model, &[10.0, 26.0, 12.0]);
        let fusion = fuse(vec![a]).unwrap();
        assert_eq!(
            fusion.series.points.iter().map(|p| p.pm2_5).collect::<Vec<_>>(),
            vec![10.0, 26.0, 12.0]
        );
        assert_eq!(fusion.spread[1], Spread { min: 26.0, max: 26.0, count: 1 });
    }

    #[test]
    fn the_reference_is_the_model_with_the_longest_horizon() {
        let model = src("model", Kind::Model, &[1.0, 2.0, 3.0]);
        let ground_short = src("ground", Kind::Ground, &[9.0]);
        let fusion = fuse(vec![model, ground_short]).unwrap();
        assert_eq!(fusion.series.len(), 3);
    }

    #[test]
    fn disagreement_across_a_band_picks_the_protective_verdict() {
        // 12 -> Open, 26 -> KeepClosed: the band straddles both thresholds.
        let a = src("a", Kind::Model, &[12.0, 14.0]);
        let b = src("b", Kind::Ground, &[26.0, 14.0]);
        let s = stack(vec![a, b], &Thresholds::default()).unwrap();
        assert_eq!(s.decision.hourly[0], Verdict::KeepClosed, "doubt -> closed");
        assert!(s.disagree[0]);
        assert_eq!(s.decision.hourly[1], Verdict::Open, "band inside one band");
        assert!(!s.disagree[1]);
    }

    #[test]
    fn agreement_within_a_band_is_not_a_disagreement() {
        let a = src("a", Kind::Model, &[16.0]);
        let b = src("b", Kind::Ground, &[24.0]);
        let s = stack(vec![a, b], &Thresholds::default()).unwrap();
        assert_eq!(s.decision.hourly[0], Verdict::Vent);
        assert!(!s.disagree[0]);
    }

    #[test]
    fn readings_now_cover_every_source() {
        let a = src("a", Kind::Model, &[10.0, 20.0, 30.0]);
        let b = src("b", Kind::Ground, &[11.0, 21.0, 31.0]);
        let s = stack(vec![a, b], &Thresholds::default()).unwrap();
        let readings = s.readings_now();
        assert_eq!(readings.len(), 2);
        assert!(readings.iter().all(|r| r.pm2_5.is_some()));
        assert!(s.multi());
    }

    #[test]
    fn no_sources_is_an_error() {
        let err = stack(vec![], &Thresholds::default()).unwrap_err();
        assert!(matches!(err, SourceError::Unavailable(_)));
    }
}
