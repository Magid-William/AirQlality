//! The transparent rule: turn PM2.5 into an actionable open/vent/closed verdict.
//!
//! Thresholds are deliberately documented constants, not a black box — that is
//! the project's differentiator (EXP01 verdict). The bands are:
//!
//! - **Open**  — PM2.5 ≤ 15 µg/m³ (WHO 2021 24-hour guideline).
//! - **Vent**  — 15 < PM2.5 ≤ 25 µg/m³ (WHO interim target 2). Outdoor air is not
//!   clean, but indoor CO2/VOC still build up, so short ventilation bursts help.
//! - **KeepClosed** — PM2.5 > 25 µg/m³.
//!
//! Wind is carried in the model but intentionally does not change the verdict yet.

use crate::dayshape::{DayShape, HORIZON_HOURS, band_census};
use crate::model::Series;

/// The advice the app gives for one hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Open,
    Vent,
    KeepClosed,
}

impl Verdict {
    /// Short word shown as the headline (matches the `UI.md` spirit).
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Open => "OPEN",
            Verdict::Vent => "VENT",
            Verdict::KeepClosed => "KEEP CLOSED",
        }
    }

    /// One-line, human "why" — the rule stays explainable.
    pub fn advice(self) -> &'static str {
        match self {
            Verdict::Open => "Outdoor air is clean - open wide and air the room out.",
            Verdict::Vent => "Outdoor air is not clean - short ventilation bursts only (CO2 still builds up indoors).",
            Verdict::KeepClosed => "Outdoor PM2.5 is high - keep the windows closed for now.",
        }
    }
}

/// The documented boundaries between verdicts, in µg/m³.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// At or below this -> [`Verdict::Open`] (WHO 24 h guideline: 15).
    pub open_max: f32,
    /// At or below this -> [`Verdict::Vent`]; above -> [`Verdict::KeepClosed`]
    /// (WHO interim target 2: 25).
    pub vent_max: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            open_max: 15.0,
            vent_max: 25.0,
        }
    }
}

/// Map one PM2.5 reading to a verdict.
pub fn classify(pm2_5: f32, thresholds: &Thresholds) -> Verdict {
    if pm2_5 <= thresholds.open_max {
        Verdict::Open
    } else if pm2_5 <= thresholds.vent_max {
        Verdict::Vent
    } else {
        Verdict::KeepClosed
    }
}

/// A contiguous run of hours `[start, end]`, inclusive, indexed into the series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub start: usize,
    pub end: usize,
}

impl Window {
    pub fn hours(&self) -> usize {
        self.end - self.start + 1
    }
}

/// The full evaluation of a series.
#[derive(Debug, Clone)]
pub struct Decision {
    /// Verdict for every point in the series, same order.
    pub hourly: Vec<Verdict>,
    /// Verdict for the present hour.
    pub headline: Verdict,
    /// Longest run of clean (Open) hours from "now" onward, within
    /// [`HORIZON_HOURS`]; a run further out than that is **not** reported, so the
    /// banner's "in the next 24 h" claim stays true.
    pub best_window: Option<Window>,
    /// The shape of the next [`HORIZON_HOURS`] hours: how many hours are clean,
    /// mediocre and closed, and when the next clean hour starts (EXP20). A
    /// census, not a run - it describes the whole horizon, so a long clean run
    /// can no longer be reported without the dark hours that surround it.
    pub shape: DayShape,
}

/// Evaluate a series against the thresholds.
pub fn evaluate(series: &Series, thresholds: &Thresholds) -> Decision {
    let hourly: Vec<Verdict> = series
        .points
        .iter()
        .map(|p| classify(p.pm2_5, thresholds))
        .collect();

    evaluate_hourly(hourly, series.now_index)
}

/// Assemble a [`Decision`] from an already-classified hourly verdict vector.
///
/// The fusion layer (EXP19) uses this after applying its conservative
/// tie-break across sources, so the "best window" search and the day census stay
/// in one place.
pub fn evaluate_hourly(hourly: Vec<Verdict>, now_index: usize) -> Decision {
    assert!(!hourly.is_empty(), "a decision needs at least one hour");
    assert!(now_index < hourly.len(), "now_index out of range");
    let shape = band_census(&hourly, now_index);
    Decision {
        headline: hourly[now_index],
        best_window: longest_run(&hourly, now_index, Verdict::Open, HORIZON_HOURS),
        shape,
        hourly,
    }
}

/// Longest contiguous run of `want` at or after `from`, looking no further than
/// `horizon` hours ahead; earliest wins on ties.
///
/// The horizon is what keeps a `Decision` honest: a clean stretch three days out
/// is not a "best window in the next 24 h".
fn longest_run(
    hourly: &[Verdict],
    from: usize,
    want: Verdict,
    horizon: usize,
) -> Option<Window> {
    let end = (from + horizon).min(hourly.len());
    let mut best: Option<Window> = None;
    let mut run_start: Option<usize> = None;

    for i in from..end {
        if hourly[i] == want {
            run_start.get_or_insert(i);
        } else if let Some(start) = run_start.take() {
            best = keep_longer(best, Window { start, end: i - 1 });
        }
    }
    if let Some(start) = run_start {
        best = keep_longer(best, Window { start, end: end - 1 });
    }
    best
}

fn keep_longer(best: Option<Window>, candidate: Window) -> Option<Window> {
    match best {
        Some(b) if b.hours() >= candidate.hours() => Some(b),
        _ => Some(candidate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{AirQualitySource, MockSource};

    #[test]
    fn thresholds_are_inclusive_lower_band() {
        let t = Thresholds::default();
        assert_eq!(classify(15.0, &t), Verdict::Open);
        assert_eq!(classify(15.1, &t), Verdict::Vent);
        assert_eq!(classify(25.0, &t), Verdict::Vent);
        assert_eq!(classify(25.1, &t), Verdict::KeepClosed);
    }

    #[test]
    fn mock_series_is_well_formed() {
        let series = MockSource.fetch().unwrap();
        assert_eq!(series.len(), 48);
        assert_eq!(series.now_index, 23);
        assert!(series
            .points
            .windows(2)
            .all(|w| w[1].time - w[0].time == chrono::Duration::hours(1)));
    }

    #[test]
    fn decision_from_mock_finds_the_window() {
        let series = MockSource.fetch().unwrap();
        let decision = evaluate(&series, &Thresholds::default());

        assert_eq!(decision.hourly.len(), 48);
        // "now" (index 23) is 26 µg/m³ -> closed.
        assert_eq!(decision.headline, Verdict::KeepClosed);

        let window = decision.best_window.expect("mock has a clean window");
        assert_eq!(window.start, 35);
        assert_eq!(window.end, 39);
        assert_eq!(window.hours(), 5);
    }

    #[test]
    fn no_open_run_reports_none() {
        let all_dirty = [Verdict::Vent, Verdict::KeepClosed, Verdict::Vent];
        assert_eq!(
            longest_run(&all_dirty, 0, Verdict::Open, HORIZON_HOURS),
            None
        );
    }

    #[test]
    fn a_clean_run_beyond_the_horizon_is_not_a_best_window() {
        // 24 dirty hours, then a long clean run: inside the series, outside the
        // next day, so the banner must not offer it as a window "in the next
        // 24 h" (EXP20).
        let mut hourly = vec![Verdict::KeepClosed; 24];
        hourly.extend(std::iter::repeat(Verdict::Open).take(6));
        assert_eq!(longest_run(&hourly, 0, Verdict::Open, HORIZON_HOURS), None);
        assert_eq!(
            longest_run(&hourly, 0, Verdict::Open, 30),
            Some(Window { start: 24, end: 29 }),
            "widen the horizon and the run appears"
        );
    }

    #[test]
    fn a_run_clipped_by_the_horizon_ends_at_the_horizon() {
        // Clean hours start 20 h out, so the horizon sees the first four.
        let mut hourly = vec![Verdict::KeepClosed; 20];
        hourly.extend(std::iter::repeat(Verdict::Open).take(10));
        assert_eq!(
            longest_run(&hourly, 0, Verdict::Open, HORIZON_HOURS),
            Some(Window { start: 20, end: 23 })
        );
    }
}
