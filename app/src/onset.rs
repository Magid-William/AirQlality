//! Warning plans (EXP30): one balloon per run, timed by the data's clock.
//!
//! The timeline is hourly, so a "bad air in 10 minutes" event is the next
//! hourly verdict boundary - a transition the forecast already contains. This
//! module decides *when* to say so: `lead` seconds before the boundary, not when
//! a fetch happens to land.
//!
//! One warning per run is kept from EXP27, but the identity is now the run's
//! **absolute hour range** instead of a state machine over evaluations: a
//! re-drawn forecast that merely extends or shortens the same spell overlaps the
//! warned range and stays silent, while a genuinely new spell starts at hours
//! that were never warned.
//!
//! Pure (no clock, no Win32), so the timing contract is unit-tested everywhere;
//! `ui.rs` supplies the evaluation instant and the tray transports the balloon.

use airqlality_core::{Decision, Series, Verdict};
use chrono::{DateTime, FixedOffset};

use crate::notify::Targets;

/// The default lead: tell the user ten minutes before the air changes.
pub const DEFAULT_LEAD_SECS: i64 = 10 * 60;

/// How a warning relates to the run it describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The run has not started yet: whole minutes until it does.
    Ahead(i64),
    /// The run starts now (the evaluation is within the first minute).
    Starting,
    /// The run is already under way: whole minutes since it started.
    Late(i64),
}

impl Phase {
    /// The word the dump records (EXP30).
    pub fn kind(self) -> &'static str {
        match self {
            Phase::Ahead(_) => "ahead",
            Phase::Starting => "now",
            Phase::Late(_) => "late",
        }
    }
}

/// One balloon to send: which verdict, when its run starts, and how far from
/// that start the evaluation is.
#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub verdict: Verdict,
    /// The run's first hour, as an absolute unix-hour bucket (the dump's `hour`).
    pub start_hour: i64,
    /// The run's first point's wall clock, for the message's "from 4:00 PM".
    pub start: DateTime<FixedOffset>,
    /// The run's first hour's reading (what the user is being warned about).
    pub pm2_5: f32,
    pub phase: Phase,
}

/// A run already warned about, as the absolute hour range it covered then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Warned {
    verdict: Verdict,
    start_hour: i64,
    end_hour: i64,
}

/// The notification planner. One per session; reset on a location switch.
#[derive(Debug, Clone, Default)]
pub struct Planner {
    warned: Vec<Warned>,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }

    /// A different place is a different run (EXP17): forget every warning.
    pub fn reset(&mut self) {
        self.warned.clear();
    }

    /// Observe the series at `now` and return the balloon to send, if any.
    ///
    /// `series.now_index` must already be anchored to `now` (the UI's
    /// `reanchor` does that) and `decision` must be the evaluation of that
    /// series. `now` is the data clock, not the fetch time.
    pub fn poll(
        &mut self,
        series: &Series,
        decision: &Decision,
        now: DateTime<FixedOffset>,
        targets: &Targets,
        lead_secs: i64,
    ) -> Option<Warning> {
        let n = series.len().min(decision.hourly.len());
        if n == 0 {
            return None;
        }
        let now_index = series.now_index.min(n - 1);
        self.prune(hour_of(series, now_index));

        // 1. The run we are already in: a cold start (app launched mid-run), a
        // wake-up, or a revised forecast that turned the current hour bad. It
        // gets one warning, late but honest, unless this run was already
        // warned (e.g. ahead of time, or on an earlier tick).
        let current = decision.hourly[now_index];
        if targets.allows(current) {
            let (start_i, end_i) = run_bounds(&decision.hourly, now_index);
            if !self.warned_before(current, hour_of(series, start_i), hour_of(series, end_i)) {
                return Some(self.record(series, current, start_i, end_i, now));
            }
        }

        // 2. The next achieved transition after now, skipping verdicts the user
        // did not choose and runs already warned about. Only the earliest one
        // is planned: later events are planned when they become next.
        let mut i = now_index + 1;
        while i < n {
            if decision.hourly[i] == decision.hourly[i - 1] {
                i += 1;
                continue;
            }
            let (start_i, end_i) = run_bounds(&decision.hourly, i);
            let verdict = decision.hourly[start_i];
            if targets.allows(verdict)
                && !self.warned_before(verdict, hour_of(series, start_i), hour_of(series, end_i))
            {
                let start = series.points[start_i].time;
                let secs_until = start.timestamp() - now.timestamp();
                if secs_until <= lead_secs {
                    return Some(self.record(series, verdict, start_i, end_i, now));
                }
                // Not due yet: the next enabled run is the one we wait for.
                return None;
            }
            i = end_i + 1;
        }
        None
    }

    fn warned_before(&self, verdict: Verdict, start_hour: i64, end_hour: i64) -> bool {
        self.warned
            .iter()
            .any(|w| w.verdict == verdict && w.start_hour <= end_hour && start_hour <= w.end_hour)
    }

    fn record(
        &mut self,
        series: &Series,
        verdict: Verdict,
        start_i: usize,
        end_i: usize,
        now: DateTime<FixedOffset>,
    ) -> Warning {
        let start_hour = hour_of(series, start_i);
        let end_hour = hour_of(series, end_i);
        self.warned.push(Warned {
            verdict,
            start_hour,
            end_hour,
        });
        let start = series.points[start_i].time;
        let secs = start.timestamp() - now.timestamp();
        let phase = if secs > 0 {
            Phase::Ahead(ceil_minutes(secs))
        } else if -secs < 60 {
            Phase::Starting
        } else {
            Phase::Late(ceil_minutes(-secs))
        };
        Warning {
            verdict,
            start_hour,
            start,
            pm2_5: series.points[start_i].pm2_5,
            phase,
        }
    }

    /// Forget ranges that can no longer describe anything: the series is at
    /// most a few days long, and the list is capped so a long session cannot
    /// grow it without bound.
    fn prune(&mut self, now_hour: i64) {
        self.warned.retain(|w| w.end_hour >= now_hour - 24 * 7);
        while self.warned.len() > 16 {
            self.warned.remove(0);
        }
    }
}

/// Whole minutes, rounded up and never zero: "in 10 min" at 09:59.5, not "0".
fn ceil_minutes(secs: i64) -> i64 {
    ((secs.max(0) + 59) / 60).max(1)
}

fn hour_of(series: &Series, index: usize) -> i64 {
    series.points[index].time.timestamp().div_euclid(3600)
}

/// The inclusive index range of the contiguous run containing `at`.
fn run_bounds(hourly: &[Verdict], at: usize) -> (usize, usize) {
    let verdict = hourly[at];
    let mut start = at;
    while start > 0 && hourly[start - 1] == verdict {
        start -= 1;
    }
    let mut end = at;
    while end + 1 < hourly.len() && hourly[end + 1] == verdict {
        end += 1;
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airqlality_core::{
        AirQualityPoint, AirQualitySource, MockSource, Thresholds, evaluate, now_index_for,
    };
    use chrono::{FixedOffset, TimeZone};

    use Verdict::KeepClosed as Red;

    fn base() -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 1, 14, 0, 0)
            .single()
            .unwrap()
    }

    /// A series whose "now" is its first point (14:00 local).
    fn series(pm: &[f32]) -> Series {
        let points = pm
            .iter()
            .enumerate()
            .map(|(hour, pm2_5)| AirQualityPoint {
                time: base() + chrono::Duration::hours(hour as i64),
                pm2_5: *pm2_5,
                pm10: pm2_5 * 1.6,
                us_aqi: pm2_5 * 4.0,
                wind_speed: 0.0,
            })
            .collect();
        Series::new("test", points, 0)
    }

    /// Anchor `pm` at `minutes` after 14:00, evaluate, and poll.
    fn poll_at(
        planner: &mut Planner,
        pm: &[f32],
        minutes: i64,
        targets: &Targets,
        lead_secs: i64,
    ) -> Option<Warning> {
        let mut s = series(pm);
        let now = base() + chrono::Duration::minutes(minutes);
        s.now_index = now_index_for(&s.points, now);
        let decision = evaluate(&s, &Thresholds::default());
        planner.poll(&s, &decision, now, targets, lead_secs)
    }

    fn closed_only() -> Targets {
        Targets::CLOSED_ONLY
    }

    const LEAD: i64 = 600;

    #[test]
    fn warns_ten_minutes_before_a_closed_run() {
        let mut planner = Planner::new();
        let pm = [12.0, 30.0, 30.0];

        // 14:49 -> still 11 min to go.
        assert!(poll_at(&mut planner, &pm, 49, &closed_only(), LEAD).is_none());
        // 14:50 -> the lead window opens.
        let warning = poll_at(&mut planner, &pm, 50, &closed_only(), LEAD)
            .expect("warn 10 min before the 15:00 run");
        assert_eq!(warning.verdict, Red);
        assert_eq!(warning.phase, Phase::Ahead(10));
        assert_eq!(warning.start, base() + chrono::Duration::hours(1));
        assert_eq!(warning.pm2_5, 30.0);
        // Every later tick, and the start itself, stays silent.
        for minutes in [51, 55, 59, 60, 61, 90, 179] {
            assert!(
                poll_at(&mut planner, &pm, minutes, &closed_only(), LEAD).is_none(),
                "a warned run must stay silent at {minutes} min"
            );
        }
    }

    #[test]
    fn a_long_run_is_one_warning() {
        let mut planner = Planner::new();
        let pm = [12.0, 30.0, 30.0, 30.0, 30.0];
        assert!(poll_at(&mut planner, &pm, 50, &closed_only(), LEAD).is_some());
        for minutes in [60, 120, 179, 240, 300] {
            assert!(poll_at(&mut planner, &pm, minutes, &closed_only(), LEAD).is_none());
        }
    }

    #[test]
    fn a_cold_start_in_bad_air_warns_once_with_since() {
        let mut planner = Planner::new();
        // 14:00 is already closed; at 14:07 the app starts.
        let warning = poll_at(&mut planner, &[30.0, 30.0, 30.0], 7, &closed_only(), LEAD)
            .expect("cold start into bad air must warn");
        assert_eq!(warning.phase, Phase::Late(7));
        assert!(poll_at(&mut planner, &[30.0, 30.0, 30.0], 8, &closed_only(), LEAD).is_none());
    }

    #[test]
    fn a_new_run_after_a_clean_hour_warns_again() {
        let mut planner = Planner::new();
        // 15:00 open, 16:00 closed, 17:00 open (the clean hour between), 18:00
        // closed again.
        let pm = [12.0, 12.0, 30.0, 12.0, 30.0];
        assert!(poll_at(&mut planner, &pm, 110, &closed_only(), LEAD).is_some(), "16:00 run");
        assert!(poll_at(&mut planner, &pm, 120, &closed_only(), LEAD).is_none());
        assert!(poll_at(&mut planner, &pm, 175, &closed_only(), LEAD).is_none());
        assert!(poll_at(&mut planner, &pm, 230, &closed_only(), LEAD).is_some(), "18:00 run");
    }

    #[test]
    fn disabled_verdicts_never_warn() {
        let mut planner = Planner::new();
        let vent_only = Targets::parse("vent");
        assert!(poll_at(&mut planner, &[12.0, 30.0, 30.0], 50, &vent_only, LEAD).is_none());
        assert!(poll_at(&mut planner, &[12.0, 20.0, 20.0], 50, &vent_only, LEAD).is_some());
        assert!(poll_at(&mut planner, &[30.0], 1, &Targets::NONE, LEAD).is_none());
    }

    #[test]
    fn the_next_enabled_run_is_found_across_a_disabled_one() {
        let mut planner = Planner::new();
        // 15:00 vent (not chosen), 16:00 closed (chosen).
        let pm = [12.0, 20.0, 30.0];
        assert!(poll_at(&mut planner, &pm, 50, &closed_only(), LEAD).is_none());
        let warning = poll_at(&mut planner, &pm, 110, &closed_only(), LEAD)
            .expect("the closed run is next even though vent comes first");
        assert_eq!(warning.verdict, Red);
        assert_eq!(warning.phase, Phase::Ahead(10));
    }

    #[test]
    fn a_run_redrawn_with_a_different_start_is_still_the_same_spell() {
        let mut planner = Planner::new();
        // Warned about 15:00-17:00 (two closed hours).
        assert!(poll_at(&mut planner, &[12.0, 30.0, 30.0], 50, &closed_only(), LEAD).is_some());
        // The next fetch shortens it to 15:00-16:00; the current hour is the
        // same spell, so it must not warn again.
        assert!(poll_at(&mut planner, &[12.0, 30.0, 12.0], 65, &closed_only(), LEAD).is_none());
    }

    #[test]
    fn the_demo_ticker_replay_does_not_warn_twice() {
        let mut planner = Planner::new();
        assert!(poll_at(&mut planner, &[12.0, 30.0], 50, &closed_only(), LEAD).is_some());
        // The mock ticker wraps `last -> 0`; the same absolute hours come back.
        assert!(poll_at(&mut planner, &[12.0, 30.0], 5, &closed_only(), LEAD).is_none());
    }

    #[test]
    fn lead_zero_warns_at_the_boundary_not_before() {
        let mut planner = Planner::new();
        assert!(poll_at(&mut planner, &[12.0, 30.0], 59, &closed_only(), 0).is_none());
        let warning = poll_at(&mut planner, &[12.0, 30.0], 60, &closed_only(), 0)
            .expect("the run starts at 15:00");
        assert_eq!(warning.phase, Phase::Starting);
    }

    #[test]
    fn sleeping_through_the_start_warns_late_once() {
        let mut planner = Planner::new();
        // No poll between 14:00 and 17:20; the run started at 15:00.
        let warning = poll_at(&mut planner, &[12.0, 30.0, 30.0, 30.0], 200, &closed_only(), LEAD)
            .expect("a wake-up inside the run must warn once");
        assert_eq!(warning.phase, Phase::Late(140));
    }

    #[test]
    fn a_short_lead_still_rounds_up_to_one_minute() {
        let mut planner = Planner::new();
        let warning = poll_at(&mut planner, &[12.0, 30.0], 55, &closed_only(), LEAD)
            .expect("5 min before the run");
        assert_eq!(warning.phase, Phase::Ahead(5));
    }

    #[test]
    fn the_mock_cycle_warns_once_per_closed_spell() {
        let series = MockSource.fetch().unwrap();
        let decision = evaluate(&series, &Thresholds::default());
        let n = series.len();

        let mut planner = Planner::new();
        let mut fires = 0;
        let mut index = series.now_index;
        for _ in 0..n {
            let mut s = series.clone();
            s.now_index = index;
            let now = s.points[index].time;
            if planner
                .poll(&s, &decision, now, &closed_only(), LEAD)
                .is_some()
            {
                fires += 1;
            }
            index = (index + 1) % n;
        }

        // Closed runs: {0}, {20..=23}, {46, 47}; one warning each. The wrap
        // from 47 to 0 is the same absolute hour range on replay, so it stays
        // silent (EXP17's wrap contract).
        assert_eq!(fires, 3, "expected one warning per closed spell");
    }

    #[test]
    fn verdicts_and_phases_are_named_for_the_dump() {
        assert_eq!(Phase::Ahead(10).kind(), "ahead");
        assert_eq!(Phase::Starting.kind(), "now");
        assert_eq!(Phase::Late(3).kind(), "late");
    }

    #[test]
    fn targets_still_speak_only_for_their_own_verdicts() {
        let both = Targets::parse("closed,vent");
        let mut planner = Planner::new();
        let pm = [12.0, 20.0, 30.0];
        assert!(poll_at(&mut planner, &pm, 50, &both, LEAD).is_some(), "vent at 15:00");
        assert!(poll_at(&mut planner, &pm, 110, &both, LEAD).is_some(), "closed at 16:00");
    }
}
