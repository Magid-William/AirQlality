//! The shape of the next day: how many clean, mediocre and closed hours are
//! ahead, and when the next clean one starts (EXP20).
//!
//! [`crate::decision::longest_run`] answers "is there a run of clean hours, and
//! where?", and over 48 h of hourly verdicts that single span answers the
//! question badly. A real Cairo evening printed `best window 03:00 - 21:00
//! (19 h)` while the coming night - the worst air of the day, as the black-cloud
//! literature and the CAMS hour-of-day means both show - went unmentioned.
//!
//! The census answers the question the banner is actually for: *how much of the
//! next day is openable, and when does the next clean hour start?* It is
//! deliberately a plain count over the verdict bands, so it can never disagree
//! with the colours the chart paints.

use chrono::Timelike;

use crate::decision::Verdict;
use crate::model::Series;

/// How many hours ahead the census looks.
pub const HORIZON_HOURS: usize = 24;

/// The day window used by the day/night wave (EXP21), in local hours: 06:00 to
/// 18:00. Documented as an *approximation* - Cairo's real sunrise and sunset
/// move this by about an hour and a half across the year, and the widget carries
/// no solar geometry. It matches the split aqi.in's day/night chart uses.
pub const DAY_START_HOUR: u32 = 6;
/// See [`DAY_START_HOUR`].
pub const DAY_END_HOUR: u32 = 18;

/// The next [`HORIZON_HOURS`] hours, counted by verdict band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DayShape {
    /// Hours the census actually saw: [`HORIZON_HOURS`], or fewer when the
    /// series ends sooner.
    pub hours_covered: usize,
    /// Hours a window can be opened (`Verdict::Open`).
    pub clean: usize,
    /// Hours worth only short ventilation bursts (`Verdict::Vent`).
    pub mediocre: usize,
    /// Hours to keep the window closed (`Verdict::KeepClosed`).
    pub closed: usize,
    /// Index into the series of the first clean hour at or after "now" within
    /// the horizon; `None` if there is no clean hour that soon.
    pub first_clean: Option<usize>,
}

impl DayShape {
    /// How many hours from "now" until the next clean hour, `0` when it is clean
    /// now.
    pub fn hours_until_first_clean(&self, now_index: usize) -> Option<usize> {
        self.first_clean.map(|i| i - now_index)
    }

    /// Whether the horizon holds nothing worth opening the window for.
    pub fn has_no_clean_hour(&self) -> bool {
        self.first_clean.is_none()
    }
}

/// Count the verdict bands from `now` (inclusive) up to [`HORIZON_HOURS`] ahead,
/// clamped at the end of the series.
pub fn band_census(hourly: &[Verdict], now_index: usize) -> DayShape {
    assert!(!hourly.is_empty(), "a census needs at least one hour");
    assert!(now_index < hourly.len(), "now_index out of range");

    let end = (now_index + HORIZON_HOURS).min(hourly.len());
    let mut shape = DayShape {
        hours_covered: end - now_index,
        clean: 0,
        mediocre: 0,
        closed: 0,
        first_clean: None,
    };

    for (offset, verdict) in hourly[now_index..end].iter().enumerate() {
        match verdict {
            Verdict::Open => {
                shape.clean += 1;
                if shape.first_clean.is_none() {
                    shape.first_clean = Some(now_index + offset);
                }
            }
            Verdict::Vent => shape.mediocre += 1,
            Verdict::KeepClosed => shape.closed += 1,
        }
    }

    shape
}

/// How much of "night" a local hour is: `0.0` at noon, `1.0` at midnight, `0.5`
/// at 06:00 and 18:00 (EXP21).
///
/// A cosine rather than a step, so the shaded region has a *wave* boundary with
/// no corners, and so the shading follows the sun's arc instead of switching at
/// an arbitrary minute. `hour` may be fractional (the painter interpolates) and
/// wraps at 24.
pub fn night_amount(hour: f32) -> f32 {
    let h = hour.rem_euclid(24.0);
    let phase = std::f32::consts::PI * (h - 12.0) / 12.0;
    0.5 * (1.0 - phase.cos())
}

/// Whether a local hour is inside the documented day window ([`DAY_START_HOUR`]
/// ..[`DAY_END_HOUR`]).
pub fn is_daytime(hour: u32) -> bool {
    (DAY_START_HOUR..DAY_END_HOUR).contains(&hour)
}

/// The night weight of every point in a series, one per hour, from that point's
/// own local hour (EXP21).
pub fn night_curve(series: &Series) -> Vec<f32> {
    series
        .points
        .iter()
        .map(|p| night_amount(p.time.hour() as f32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decision::{Thresholds, evaluate};
    use crate::source::{AirQualitySource, MockSource};

    fn verdicts(tokens: &[char]) -> Vec<Verdict> {
        tokens
            .iter()
            .map(|c| match c {
                'o' => Verdict::Open,
                'v' => Verdict::Vent,
                'c' => Verdict::KeepClosed,
                other => panic!("unknown band {other}"),
            })
            .collect()
    }

    #[test]
    fn counts_the_mock_series() {
        // Mock "now" (index 23) is 26 ug/m3 -> closed. The next 24 h are
        // 23..=46: the closed current hour, eleven Vent, five Open (35..=39),
        // then Vent up to 25.5 at 46, which is above the Vent ceiling again.
        let series = MockSource.fetch().unwrap();
        let decision = evaluate(&series, &Thresholds::default());
        let shape = decision.shape;

        assert_eq!(shape.hours_covered, 24);
        assert_eq!(shape.clean, 5);
        assert_eq!(shape.mediocre, 17);
        assert_eq!(shape.closed, 2, "the current hour and the last one");
        assert_eq!(shape.first_clean, Some(35));
        assert_eq!(shape.hours_until_first_clean(series.now_index), Some(12));
        assert_eq!(
            shape.clean + shape.mediocre + shape.closed,
            shape.hours_covered
        );
    }

    #[test]
    fn clamps_at_the_end_of_a_short_series() {
        let shape = band_census(&verdicts(&['c', 'v', 'o']), 0);
        assert_eq!(shape.hours_covered, 3);
        assert_eq!((shape.clean, shape.mediocre, shape.closed), (1, 1, 1));
    }

    #[test]
    fn an_all_clean_day_has_no_closed_hours() {
        let shape = band_census(&verdicts(&['o'; 30]), 0);
        assert_eq!(shape.clean, 24);
        assert_eq!((shape.mediocre, shape.closed), (0, 0));
        assert_eq!(shape.first_clean, Some(0), "it is clean now");
        assert_eq!(shape.hours_until_first_clean(0), Some(0));
        assert!(!shape.has_no_clean_hour());
    }

    #[test]
    fn a_clean_hour_beyond_the_horizon_is_not_the_first_clean_hour() {
        // 24 dirty hours, then a clean one: outside the horizon, so the census
        // must say there is no clean hour ahead.
        let mut bands = verdicts(&['c'; 24]);
        bands.push(Verdict::Open);
        let shape = band_census(&bands, 0);
        assert_eq!(shape.closed, 24);
        assert_eq!(shape.hours_covered, 24);
        assert_eq!(shape.first_clean, None);
        assert!(shape.has_no_clean_hour());
        assert_eq!(shape.hours_until_first_clean(0), None);
    }

    #[test]
    fn the_census_says_what_the_run_leaves_out() {
        // The failure this experiment exists for, in miniature: six mediocre
        // hours, eight closed hours, then a long clean run to the end of the
        // horizon. `longest_run` reports only "10 clean hours" - the census says
        // how much of the day is *not* openable, and how far away the clean ones
        // are.
        let bands = verdicts(&['v'; 6])
            .into_iter()
            .chain(verdicts(&['c'; 8]))
            .chain(verdicts(&['o'; 10]))
            .collect::<Vec<_>>();
        let shape = band_census(&bands, 0);
        assert_eq!((shape.clean, shape.mediocre, shape.closed), (10, 6, 8));
        assert_eq!(shape.first_clean, Some(14));
        assert_eq!(shape.hours_until_first_clean(0), Some(14));

        let decision = crate::decision::evaluate_hourly(bands, 0);
        let window = decision.best_window.expect("a long clean run exists");
        assert_eq!(window.hours(), 10, "the run alone hides the 14 dark hours");
    }

    #[test]
    #[should_panic(expected = "now_index out of range")]
    fn rejects_an_out_of_range_now() {
        band_census(&verdicts(&['o']), 1);
    }

    #[test]
    fn the_wave_is_anchored_at_noon_midnight_and_the_edges_of_the_day() {
        assert_eq!(night_amount(12.0), 0.0, "noon is the flat top");
        assert_eq!(night_amount(0.0), 1.0, "midnight is the peak");
        assert_eq!(night_amount(24.0), 1.0, "and it wraps");
        // Dawn and dusk are the middle of the ramp, by construction.
        assert!((night_amount(6.0) - 0.5).abs() < 1e-6);
        assert!((night_amount(18.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_wave_rises_from_noon_to_midnight_with_no_step() {
        // Monotone and smooth: no hour jumps more than a cosine sample would.
        let mut previous = night_amount(12.0);
        for step in 1..=12 {
            let value = night_amount(12.0 + step as f32);
            assert!(
                value > previous,
                "the wave must keep rising: {value} after {previous}"
            );
            assert!(
                value - previous < 0.2,
                "a step, not a wave: {value} after {previous}"
            );
            previous = value;
        }
        assert_eq!(previous, night_amount(24.0));

        // ...and falls again on the other side.
        assert!(night_amount(1.0) > night_amount(3.0));
        assert!(night_amount(3.0) > night_amount(6.0));
    }

    #[test]
    fn the_day_window_is_the_documented_one() {
        assert!(is_daytime(DAY_START_HOUR));
        assert!(is_daytime(12));
        assert!(!is_daytime(DAY_END_HOUR));
        assert!(!is_daytime(5));
        assert!(!is_daytime(23));
    }

    #[test]
    fn the_curve_follows_each_point_s_own_local_hour_across_a_day_boundary() {
        // The mock starts at 14:00 local: the weight must rise to its peak at the
        // 00:00 point and be back at its floor by the next 12:00.
        let series = MockSource.fetch().unwrap();
        let curve = night_curve(&series);
        assert_eq!(curve.len(), series.len());

        let weight_at = |hour: u32| {
            let i = series
                .points
                .iter()
                .position(|p| p.time.hour() == hour)
                .unwrap_or_else(|| panic!("no point at {hour}:00"));
            curve[i]
        };
        assert_eq!(weight_at(12), 0.0);
        assert_eq!(weight_at(0), 1.0);
        let peak = curve.iter().copied().fold(f32::MIN, f32::max);
        let floor = curve.iter().copied().fold(f32::MAX, f32::min);
        assert_eq!(peak, 1.0, "midnight is the peak of the wave");
        assert_eq!(floor, 0.0, "noon is the floor");
        assert!(weight_at(18) > weight_at(15), "the evening ramps up");
    }
}
