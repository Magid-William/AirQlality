//! The "should we warn?" gate for the system notification (EXP17).
//!
//! The widget exists to tell you when to open the window; the one moment it
//! should actively interrupt you is when the outdoor air turns bad enough that
//! an open window is a mistake. So a notification is **edge-triggered** on
//! entering [`Verdict::KeepClosed`]: never for `Open`/`Vent`, and never twice
//! for the same closed spell.
//!
//! Kept free of Win32 (and of any global state) so the transition contract is
//! unit-tested on every platform; `tray.rs` owns the shell call.

use airqlality_core::Verdict;

/// The main message: the imperative, uppercase so it reads first and fast.
pub const HEADLINE: &str = "KEEP THE WINDOW CLOSED";

/// Whether a freshly evaluated `current` verdict should raise a notification.
///
/// `previous` is the verdict of the previous evaluation, or `None` before the
/// first one - so a cold start that is already `KeepClosed` warns exactly once.
pub fn should_notify(previous: Option<Verdict>, current: Verdict) -> bool {
    current == Verdict::KeepClosed && previous != Some(Verdict::KeepClosed)
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

    #[test]
    fn warns_only_on_entering_keep_closed() {
        // A cold start already closed is the main case: warn once.
        assert!(should_notify(None, Verdict::KeepClosed));
        assert!(should_notify(Some(Verdict::Open), Verdict::KeepClosed));
        assert!(should_notify(Some(Verdict::Vent), Verdict::KeepClosed));
    }

    #[test]
    fn never_warns_for_open_or_vent() {
        assert!(!should_notify(None, Verdict::Open));
        assert!(!should_notify(None, Verdict::Vent));
        assert!(!should_notify(Some(Verdict::KeepClosed), Verdict::Open));
        assert!(!should_notify(Some(Verdict::KeepClosed), Verdict::Vent));
        assert!(!should_notify(Some(Verdict::Vent), Verdict::Vent));
        assert!(!should_notify(Some(Verdict::Open), Verdict::Open));
    }

    #[test]
    fn stays_quiet_while_it_keeps_being_closed() {
        // The 30 min refresh (and every 3 s demo tick) re-evaluates the same
        // closed verdict; that must not fire again.
        assert!(!should_notify(Some(Verdict::KeepClosed), Verdict::KeepClosed));
    }

    #[test]
    fn a_new_closed_spell_warns_again() {
        // Closed -> Open -> Closed is two distinct spells, so two warnings.
        let mut previous = None;
        let mut fires = 0;
        for verdict in [
            Verdict::KeepClosed,
            Verdict::KeepClosed,
            Verdict::Vent,
            Verdict::Open,
            Verdict::KeepClosed,
        ] {
            if should_notify(previous, verdict) {
                fires += 1;
            }
            previous = Some(verdict);
        }
        assert_eq!(fires, 2);
    }

    #[test]
    fn mock_cycle_warns_once_per_closed_spell() {
        use airqlality_core::{AirQualitySource, MockSource, Thresholds, evaluate};

        let series = MockSource.fetch().unwrap();
        let decision = evaluate(&series, &Thresholds::default());
        let n = decision.hourly.len();

        // Walk one full 48 h cycle from "now" (the ticker's `last -> 0` wrap is
        // the end-of-timeline case), counting notifications.
        let mut previous = None;
        let mut fires = 0;
        let mut index = series.now_index;
        for _ in 0..n {
            let current = decision.hourly[index];
            if should_notify(previous, current) {
                fires += 1;
            }
            previous = Some(current);
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
        assert_eq!(title, title.to_uppercase(), "the main message must be uppercase");
        // The body is only the data: location + reading, no imperative.
        assert!(body.contains("Cairo"), "body: {body}");
        assert!(body.contains("26"), "body: {body}");
        assert!(
            !body.to_uppercase().contains("KEEP THE WINDOW"),
            "the imperative must not be repeated in the body: {body}"
        );
    }
}
