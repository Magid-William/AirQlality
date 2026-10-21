//! The "should we warn?" gate for the system notification (EXP17, EXP27).
//!
//! The widget exists to tell you when to open the window; the one moment it
//! should actively interrupt you is when the outdoor air turns bad enough that
//! an open window is a mistake. So a notification is **edge-triggered** on
//! entering [`Verdict::KeepClosed`]: never for `Open`/`Vent`, and never twice
//! for the same closed run.
//!
//! EXP27 tightened "the same run": the gate now keys on the **hour bucket** of
//! the reading, so a 30 min refresh that momentarily reports `Vent` and then
//! `KeepClosed` again inside one hour is still one run - it is one red segment
//! on the timeline - and stays silent. Re-arming needs a whole non-red hour
//! that is strictly later than the warning, which is exactly what a new red
//! segment on the timeline looks like.
//!
//! Kept free of Win32 (and of any global state) so the transition contract is
//! unit-tested on every platform; `tray.rs` owns the shell call.

use airqlality_core::Verdict;

/// The main message: the imperative, uppercase so it reads first and fast.
pub const HEADLINE: &str = "KEEP THE WINDOW CLOSED";

/// The notification decision, one observation at a time.
///
/// Observations are fed in the order the app evaluates them. `hour` is the
/// unix-hour bucket of the reading being evaluated (the chart is hourly), not
/// the wall clock: a refresh re-evaluating the same hour must not look like a
/// new segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    /// Hour of the warning last sent, if any.
    warned: Option<i64>,
    /// Hour of the last non-red reading seen since that warning.
    cleared: Option<i64>,
    /// Latest hour observed, so a repeated or backwards clock cannot pretend to
    /// be a new segment (the demo ticker wraps at the end of its timeline).
    last_hour: Option<i64>,
    /// Verdict of the previous observation, for the cold-start edge.
    last: Option<Verdict>,
}

impl Gate {
    /// A fresh gate: the next closed reading warns.
    pub const fn new() -> Self {
        Self {
            warned: None,
            cleared: None,
            last_hour: None,
            last: None,
        }
    }

    /// Forget everything: a new location is a new run (EXP17).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Observe one evaluation and return whether it should raise a balloon.
    ///
    /// - the first closed reading of a session warns (cold start into bad air);
    /// - a long closed run warns once, however many refreshes land inside it;
    /// - a closed reading after a **whole non-red hour** warns again;
    /// - a non-red reading inside the hour of the last warning does not re-arm
    ///   (the timeline still shows one red segment there).
    pub fn observe(&mut self, verdict: Verdict, hour: i64) -> bool {
        let backwards = self.last_hour.is_some_and(|h| hour < h);
        if backwards {
            // The timeline restarted (the demo ticker wraps `last -> 0`): it is
            // the same wall-clock moment, not a new segment - so forget the
            // markers but keep the previous verdict for the edge.
            self.warned = None;
            self.cleared = None;
        }
        let hour = match self.last_hour {
            Some(h) if !backwards => h.max(hour),
            _ => hour,
        };
        self.last_hour = Some(hour);

        let previous = self.last;
        self.last = Some(verdict);

        if verdict != Verdict::KeepClosed {
            if self.warned.is_some() {
                self.cleared = Some(hour);
            }
            return false;
        }

        let fresh = match self.warned {
            // Cold start, or the first closed reading after a location switch.
            None => previous != Some(Verdict::KeepClosed),
            // A new segment needs a whole non-red hour *between* the two runs.
            Some(warned) => self
                .cleared
                .is_some_and(|cleared| cleared > warned && hour > cleared),
        };

        if fresh {
            self.warned = Some(hour);
            self.cleared = None;
        }
        fresh
    }
}

/// The title/body for a `KeepClosed` warning: the main message first, the
/// supporting data after it.
pub fn message(location: &str, pm2_5: f32) -> (String, String) {
    (
        HEADLINE.to_string(),
        format!("{location} \u{b7} PM2.5 {pm2_5:.0} \u{b5}g/m\u{b3}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a sequence of verdicts, one per hour, and count the warnings.
    fn warnings(sequence: &[Verdict]) -> usize {
        let mut gate = Gate::new();
        sequence
            .iter()
            .enumerate()
            .filter(|(hour, verdict)| gate.observe(**verdict, *hour as i64))
            .count()
    }

    use Verdict::{KeepClosed as Red, Open as Green, Vent as Orange};

    #[test]
    fn the_requested_pattern_warns_twice() {
        // [green][orange][red][orange][red][orange] -> 2.
        assert_eq!(
            warnings(&[Green, Orange, Red, Orange, Red, Orange]),
            2,
            "one warning per red segment"
        );
    }

    #[test]
    fn a_long_red_run_still_warns_once() {
        // Duration is irrelevant: the whole run is one segment.
        let mut sequence = vec![Green, Orange];
        sequence.extend(std::iter::repeat(Red).take(10));
        assert_eq!(warnings(&sequence), 1);

        // ... and a single red hour is also one.
        assert_eq!(warnings(&[Green, Red, Orange]), 1);
    }

    #[test]
    fn a_new_segment_separated_by_a_hour_warns_again() {
        assert_eq!(warnings(&[Red, Green, Red]), 2);
        assert_eq!(warnings(&[Red, Orange, Red]), 2);
    }

    #[test]
    fn a_cold_start_into_bad_air_warns_once() {
        assert_eq!(warnings(&[Red]), 1);
        assert_eq!(warnings(&[Red, Red, Red]), 1);
        assert_eq!(warnings(&[Green, Orange]), 0);
    }

    #[test]
    fn a_same_hour_flicker_is_one_segment() {
        // The 30 min refresh reports Vent and then KeepClosed again inside the
        // same hour: the timeline still shows one red segment, so one warning.
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 100));
        assert!(!gate.observe(Orange, 100));
        assert!(
            !gate.observe(Red, 100),
            "a flicker inside the warning hour must not warn twice"
        );
        // Repeated refreshes of the same hour stay silent too.
        assert!(!gate.observe(Red, 100));
        assert!(!gate.observe(Orange, 100));
        assert!(!gate.observe(Red, 100));
    }

    #[test]
    fn a_flicker_in_the_hour_after_the_warning_does_not_rearm() {
        // Closed at hour 2, Vent then Closed at hour 3: the run is continuous
        // (hours 2-3 are both red on the timeline), so still one warning.
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 2));
        assert!(!gate.observe(Orange, 3));
        assert!(!gate.observe(Red, 3));
    }

    #[test]
    fn a_whole_non_red_hour_between_two_runs_rearms() {
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 2));
        assert!(!gate.observe(Orange, 3));
        assert!(!gate.observe(Orange, 4));
        assert!(
            gate.observe(Red, 5),
            "hours 3 and 4 were not red, so hour 5 starts a new segment"
        );
    }

    #[test]
    fn open_and_vent_never_warn() {
        let mut gate = Gate::new();
        for (hour, verdict) in [Green, Orange, Green, Orange, Orange].iter().enumerate() {
            assert!(!gate.observe(*verdict, hour as i64));
        }
    }

    #[test]
    fn the_demo_timeline_wrap_is_not_a_new_segment() {
        // The ticker wraps `last -> 0`, so the hour bucket jumps backwards while
        // the verdict carries on. That must not re-warn (EXP17 contract).
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 47));
        assert!(!gate.observe(Red, 0), "wrapping is not a new red run");
        assert!(!gate.observe(Orange, 1));
        assert!(
            gate.observe(Red, 2),
            "hour 1 was not red, so hour 2 starts a new segment"
        );
    }

    #[test]
    fn reset_forgets_the_last_run() {
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 10));
        assert!(!gate.observe(Red, 11));
        gate.reset();
        assert!(gate.observe(Red, 11), "a location switch is a new run");
    }

    #[test]
    fn mock_cycle_warns_once_per_closed_spell() {
        use airqlality_core::{AirQualitySource, MockSource, Thresholds, evaluate};

        let series = MockSource.fetch().unwrap();
        let decision = evaluate(&series, &Thresholds::default());
        let n = decision.hourly.len();

        // Walk one full 48 h cycle from "now" (the ticker's `last -> 0` wrap is
        // the end-of-timeline case), counting notifications.
        let mut gate = Gate::new();
        let mut fires = 0;
        let mut index = series.now_index;
        for _ in 0..n {
            let hour = series.points[index].time.timestamp() / 3600;
            if gate.observe(decision.hourly[index], hour) {
                fires += 1;
            }
            index = (index + 1) % n;
        }

        // Mock PM2.5 >25 runs: {0}, {20..=23}, {46, 47}; the last and first are
        // the wrap-adjacent closed spell, so one cycle warns 3 times.
        assert_eq!(fires, 3, "expected one warning per closed spell");
    }

    #[test]
    fn message_leads_with_the_uppercase_imperative() {
        let (title, body) = message("Cairo", 26.0);
        // Main message first, uppercase; supporting data after.
        assert_eq!(title, "KEEP THE WINDOW CLOSED");
        assert_eq!(
            title,
            title.to_uppercase(),
            "the main message must be uppercase"
        );
        // The body is only the data: location + reading, no imperative.
        assert!(body.contains("Cairo"), "body: {body}");
        assert!(body.contains("26"), "body: {body}");
        assert!(
            !body.to_uppercase().contains("KEEP THE WINDOW"),
            "the imperative must not be repeated in the body: {body}"
        );
    }
}
