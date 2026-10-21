//! The "should we warn?" gate for the system notification (EXP17, EXP27, EXP28).
//!
//! The widget exists to tell you when to open the window; the moments it should
//! actively interrupt you are the ones you asked for. By default that is only
//! entering [`Verdict::KeepClosed`] - an open window in bad air is the mistake
//! the app exists to prevent - but the tray's "Notify me on" submenu lets the
//! user add opening and venting too (EXP28).
//!
//! A notification is **edge-triggered** on entering an enabled verdict: never
//! twice for the same run, and never for a verdict the user did not pick.
//! EXP27 tightened "the same run": the gate keys on the **hour bucket** of the
//! reading, so a 30 min refresh that momentarily reports another verdict and
//! then the same one again inside one hour is still one run - one segment on the
//! timeline - and stays silent. Re-arming needs a whole other hour between two
//! runs.
//!
//! Kept free of Win32 (and of any global state) so the transition contract is
//! unit-tested on every platform; `tray.rs` owns the shell call.

use airqlality_core::Verdict;

/// The closed warning: the imperative, uppercase so it reads first and fast.
pub const HEADLINE_CLOSED: &str = "KEEP THE WINDOW CLOSED";
/// The clean-air notification (EXP28): the mirror of the closed warning.
pub const HEADLINE_OPEN: &str = "OPEN THE WINDOW";
/// The mediocre-air notification (EXP28), matching the in-app advice that only
/// short ventilation bursts help.
pub const HEADLINE_VENT: &str = "VENTILATE BRIEFLY";

/// Which verdicts may raise a notification, chosen in the tray submenu (EXP28).
///
/// Stored as text in `%APPDATA%\AirQlality\notify.txt`. The default is the
/// pre-EXP28 behaviour: only a closed window warns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Targets {
    closed: bool,
    vent: bool,
    open: bool,
}

impl Default for Targets {
    fn default() -> Self {
        Self::CLOSED_ONLY
    }
}

impl Targets {
    /// The default and pre-EXP28 behaviour: warn when the window should close.
    pub const CLOSED_ONLY: Self = Self {
        closed: true,
        vent: false,
        open: false,
    };

    /// Nothing selected: the app never interrupts. An explicit choice, not an
    /// error.
    pub const NONE: Self = Self {
        closed: false,
        vent: false,
        open: false,
    };

    pub fn closed(&self) -> bool {
        self.closed
    }

    pub fn vent(&self) -> bool {
        self.vent
    }

    pub fn open(&self) -> bool {
        self.open
    }

    /// Whether a reading of `verdict` should be able to warn.
    pub fn allows(&self, verdict: Verdict) -> bool {
        match verdict {
            Verdict::KeepClosed => self.closed,
            Verdict::Vent => self.vent,
            Verdict::Open => self.open,
        }
    }

    /// Flip one choice on or off (the tray menu's toggle).
    pub fn toggle(&mut self, verdict: Verdict) {
        let slot = match verdict {
            Verdict::KeepClosed => &mut self.closed,
            Verdict::Vent => &mut self.vent,
            Verdict::Open => &mut self.open,
        };
        *slot = !*slot;
    }

    /// Nothing can warn.
    #[cfg(test)]
    pub fn is_none(&self) -> bool {
        !self.closed && !self.vent && !self.open
    }

    /// Parse the stored text: `closed,vent,open` in any order, or `none`.
    ///
    /// Unknown tokens are ignored rather than fatal, so a hand-edited file
    /// cannot break notifications entirely. A blank value is `none`: an empty
    /// file is an explicit silence, not a reason to fall back to the default.
    pub fn parse(text: &str) -> Self {
        let mut targets = Self::NONE;
        for token in text.split(',') {
            match token.trim().to_ascii_lowercase().as_str() {
                "closed" | "closing" => targets.closed = true,
                "vent" | "venting" => targets.vent = true,
                "open" | "opening" => targets.open = true,
                _ => {}
            }
        }
        targets
    }

    /// The stored text, in the menu's order; `none` when nothing is selected.
    pub fn to_text(self) -> String {
        let mut parts = Vec::new();
        if self.closed {
            parts.push("closed");
        }
        if self.open {
            parts.push("open");
        }
        if self.vent {
            parts.push("vent");
        }
        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(",")
        }
    }
}

/// The notification decision, one observation at a time.
///
/// Observations are fed in the order the app evaluates them. `hour` is the
/// unix-hour bucket of the reading being evaluated (the chart is hourly), not
/// the wall clock: a refresh re-evaluating the same hour must not look like a
/// new segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    /// The verdict and hour of the warning last sent, if any.
    fired: Option<(Verdict, i64)>,
    /// Hour of the last reading with a *different* verdict than the warning
    /// verdict, since that warning.
    other: Option<i64>,
    /// Latest hour observed, so a repeated or backwards clock cannot pretend to
    /// be a new segment (the demo ticker wraps at the end of its timeline).
    last_hour: Option<i64>,
    /// Verdict of the previous observation, for the cold-start edge.
    last: Option<Verdict>,
}

impl Gate {
    /// A fresh gate: the next enabled reading warns.
    pub const fn new() -> Self {
        Self {
            fired: None,
            other: None,
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
    /// - the first enabled reading of a session warns (cold start into bad air -
    ///   or into clean air, if the user asked to be told about that);
    /// - a long run warns once, however many refreshes land inside it;
    /// - a new run of the same enabled verdict warns after a whole other hour;
    /// - a different enabled verdict warns when its run starts in a later hour;
    /// - a non-enabled verdict never warns.
    pub fn observe(&mut self, verdict: Verdict, hour: i64, targets: &Targets) -> bool {
        let backwards = self.last_hour.is_some_and(|h| hour < h);
        if backwards {
            // The timeline restarted (the demo ticker wraps `last -> 0`): it is
            // the same wall-clock moment, not a new segment - so forget the
            // markers but keep the previous verdict for the edge.
            self.fired = None;
            self.other = None;
        }
        let hour = match self.last_hour {
            Some(h) if !backwards => h.max(hour),
            _ => hour,
        };
        self.last_hour = Some(hour);

        let previous = self.last;
        self.last = Some(verdict);

        if let Some((fired, _)) = self.fired {
            if verdict != fired {
                self.other = Some(hour);
            }
        }

        let fresh = targets.allows(verdict)
            && match self.fired {
                // Cold start, or the first enabled reading after a reset.
                None => previous != Some(verdict),
                // A different verdict starting in a later hour is a new run.
                Some((fired, warned)) if verdict != fired => hour > warned,
                // The same verdict needs a whole other hour in between.
                Some((_, warned)) => self
                    .other
                    .is_some_and(|other| other > warned && hour > other),
            };

        if fresh {
            self.fired = Some((verdict, hour));
            self.other = None;
        }
        fresh
    }
}

/// The title/body for a warning: the imperative for that verdict first, the
/// supporting data after it.
pub fn message(verdict: Verdict, location: &str, pm2_5: f32) -> (String, String) {
    let title = match verdict {
        Verdict::KeepClosed => HEADLINE_CLOSED,
        Verdict::Vent => HEADLINE_VENT,
        Verdict::Open => HEADLINE_OPEN,
    };
    (
        title.to_string(),
        format!("{location} \u{b7} PM2.5 {pm2_5:.0} \u{b5}g/m\u{b3}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a sequence of verdicts, one per hour, and count the warnings.
    fn warnings_with(sequence: &[Verdict], targets: &Targets) -> usize {
        let mut gate = Gate::new();
        sequence
            .iter()
            .enumerate()
            .filter(|(hour, verdict)| gate.observe(**verdict, *hour as i64, targets))
            .count()
    }

    /// The closed-only behaviour EXP27 pinned.
    fn warnings(sequence: &[Verdict]) -> usize {
        warnings_with(sequence, &Targets::CLOSED_ONLY)
    }

    use Verdict::{KeepClosed as Red, Open as Green, Vent as Orange};

    #[test]
    fn the_requested_pattern_warns_twice_for_closing() {
        // [green][orange][red][orange][red][orange] -> 2 when only closing warns.
        assert_eq!(
            warnings(&[Green, Orange, Red, Orange, Red, Orange]),
            2,
            "one warning per red segment"
        );
    }

    #[test]
    fn every_segment_warns_when_all_three_are_enabled() {
        let all = Targets::parse("closed,vent,open");
        // Each of the six segments is one run of its own verdict.
        assert_eq!(
            warnings_with(&[Green, Orange, Red, Orange, Red, Orange], &all),
            6
        );
        // A long run of one verdict is still one.
        assert_eq!(
            warnings_with(&[Green, Orange, Red, Red, Red, Red], &all),
            3
        );
    }

    #[test]
    fn a_disabled_verdict_never_warns() {
        let vent_only = Targets::parse("vent");
        assert_eq!(warnings_with(&[Red, Red, Green, Orange], &vent_only), 1);
        let none = Targets::NONE;
        assert_eq!(warnings_with(&[Red, Orange, Green, Red], &none), 0);
    }

    #[test]
    fn a_long_run_still_warns_once() {
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
        // If the user asked about clean air too, a cold start warns for that.
        let all = Targets::parse("closed,vent,open");
        assert_eq!(warnings_with(&[Green], &all), 1);
    }

    #[test]
    fn a_same_hour_flicker_is_one_segment() {
        // The 30 min refresh reports Vent and then KeepClosed again inside the
        // same hour: the timeline still shows one red segment, so one warning.
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 100, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Orange, 100, &Targets::CLOSED_ONLY));
        assert!(
            !gate.observe(Red, 100, &Targets::CLOSED_ONLY),
            "a flicker inside the warning hour must not warn twice"
        );
        // Repeated refreshes of the same hour stay silent too.
        assert!(!gate.observe(Red, 100, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Orange, 100, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Red, 100, &Targets::CLOSED_ONLY));
    }

    #[test]
    fn a_flicker_in_the_hour_after_the_warning_does_not_rearm() {
        // Closed at hour 2, Vent then Closed at hour 3: the run is continuous
        // (hours 2-3 are both red on the timeline), so still one warning.
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 2, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Orange, 3, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Red, 3, &Targets::CLOSED_ONLY));
    }

    #[test]
    fn a_whole_other_hour_between_two_runs_rearms() {
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 2, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Orange, 3, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Orange, 4, &Targets::CLOSED_ONLY));
        assert!(
            gate.observe(Red, 5, &Targets::CLOSED_ONLY),
            "hours 3 and 4 were not red, so hour 5 starts a new segment"
        );
    }

    #[test]
    fn a_different_enabled_verdict_warns_when_its_run_starts() {
        // Closed at hour 2, Vent at hour 3: with vent enabled that is its own
        // segment, so it warns in the same observation it starts.
        let both = Targets::parse("closed,vent");
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 2, &both));
        assert!(gate.observe(Orange, 3, &both));
        assert!(
            !gate.observe(Orange, 4, &both),
            "the same vent run does not warn twice"
        );
        assert!(gate.observe(Red, 5, &both));
    }

    #[test]
    fn open_and_vent_never_warn_unless_enabled() {
        let mut gate = Gate::new();
        for (hour, verdict) in [Green, Orange, Green, Orange, Orange].iter().enumerate() {
            assert!(!gate.observe(*verdict, hour as i64, &Targets::CLOSED_ONLY));
        }
    }

    #[test]
    fn the_demo_timeline_wrap_is_not_a_new_segment() {
        // The ticker wraps `last -> 0`, so the hour bucket jumps backwards while
        // the verdict carries on. That must not re-warn (EXP17 contract).
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 47, &Targets::CLOSED_ONLY));
        assert!(
            !gate.observe(Red, 0, &Targets::CLOSED_ONLY),
            "wrapping is not a new red run"
        );
        assert!(!gate.observe(Orange, 1, &Targets::CLOSED_ONLY));
        assert!(
            gate.observe(Red, 2, &Targets::CLOSED_ONLY),
            "hour 1 was not red, so hour 2 starts a new segment"
        );
    }

    #[test]
    fn reset_forgets_the_last_run() {
        let mut gate = Gate::new();
        assert!(gate.observe(Red, 10, &Targets::CLOSED_ONLY));
        assert!(!gate.observe(Red, 11, &Targets::CLOSED_ONLY));
        gate.reset();
        assert!(
            gate.observe(Red, 11, &Targets::CLOSED_ONLY),
            "a location switch is a new run"
        );
    }

    #[test]
    fn defaults_to_closing_only() {
        let default = Targets::default();
        assert!(default.closed());
        assert!(!default.vent());
        assert!(!default.open());
        assert!(default.allows(Verdict::KeepClosed));
        assert!(!default.allows(Verdict::Vent));
        assert!(!default.allows(Verdict::Open));
    }

    #[test]
    fn targets_round_trip_through_their_text_form() {
        for text in ["closed", "closed,vent", "closed,open,vent", "none"] {
            let parsed = Targets::parse(text);
            assert_eq!(parsed.to_text(), text, "round trip of {text}");
            assert_eq!(Targets::parse(&parsed.to_text()), parsed);
        }
        // The menu order is closing, opening, venting, whatever the input order.
        assert_eq!(Targets::parse("vent, open ,closed").to_text(), "closed,open,vent");
        assert_eq!(Targets::parse("none").to_text(), "none");
    }

    #[test]
    fn parsing_is_forgiving_about_case_whitespace_and_unknown_tokens() {
        assert_eq!(Targets::parse(" Closing , VENT "), Targets::parse("closed,vent"));
        assert_eq!(Targets::parse("garbage"), Targets::NONE);
        assert_eq!(Targets::parse("closed,garbage"), Targets::parse("closed"));
        // A blank file is an explicit silence, not the default.
        assert_eq!(Targets::parse(""), Targets::NONE);
        assert_eq!(Targets::parse("   \n"), Targets::NONE);
    }

    #[test]
    fn toggling_one_verdict_leaves_the_others_alone() {
        let mut targets = Targets::CLOSED_ONLY;
        targets.toggle(Verdict::Vent);
        assert_eq!(targets.to_text(), "closed,vent");
        targets.toggle(Verdict::KeepClosed);
        assert_eq!(targets.to_text(), "vent");
        targets.toggle(Verdict::Open);
        assert_eq!(targets.to_text(), "open,vent");
        targets.toggle(Verdict::Vent);
        targets.toggle(Verdict::Open);
        assert!(targets.is_none());
        assert_eq!(targets.to_text(), "none");
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
            if gate.observe(decision.hourly[index], hour, &Targets::CLOSED_ONLY) {
                fires += 1;
            }
            index = (index + 1) % n;
        }

        // Mock PM2.5 >25 runs: {0}, {20..=23}, {46, 47}; the last and first are
        // the wrap-adjacent closed spell, so one cycle warns 3 times.
        assert_eq!(fires, 3, "expected one warning per closed spell");
    }

    #[test]
    fn message_leads_with_the_verdict_imperative() {
        let (title, body) = message(Verdict::KeepClosed, "Cairo", 26.0);
        // Main message first, uppercase; supporting data after.
        assert_eq!(title, HEADLINE_CLOSED);
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
        // EXP28: each verdict has its own imperative.
        assert_eq!(message(Verdict::Open, "Cairo", 9.0).0, HEADLINE_OPEN);
        assert_eq!(message(Verdict::Vent, "Cairo", 20.0).0, HEADLINE_VENT);
        assert_ne!(HEADLINE_OPEN, HEADLINE_VENT);
    }
}
