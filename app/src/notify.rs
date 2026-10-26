//! Which verdicts may interrupt the user, and what the balloon says
//! (EXP17, EXP27, EXP28, EXP30).
//!
//! The widget exists to tell you when to open the window; the moments it should
//! actively interrupt you are the ones you asked for. By default that is only
//! entering [`Verdict::KeepClosed`] - an open window in bad air is the mistake
//! the app exists to prevent - but the tray's "Notify me on" submenu lets the
//! user add opening and venting too (EXP28).
//!
//! EXP30 moved *when* a balloon is due out of this module and into
//! [`crate::onset`]: the warning is planned from the series' own timestamps,
//! `lead` before a run starts, instead of being triggered by a fetch that
//! re-evaluated the same hour. What remains here is the user's choice
//! ([`Targets`]) and the wording.
//!
//! Kept free of Win32 (and of any global state) so the choices and the text are
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

/// [`message_at`] with the run's timing appended (EXP30): `from 4:00 PM (in
/// 10 min)`, `from 4:00 PM`, or `since 4:00 PM (20 min ago)`.
///
/// The data prefix stays untouched, so the wording the user already knows -
/// and every assertion pinned to it - survives.
pub fn message_at(
    verdict: Verdict,
    location: &str,
    pm2_5: f32,
    timing: Option<&str>,
) -> (String, String) {
    let title = match verdict {
        Verdict::KeepClosed => HEADLINE_CLOSED,
        Verdict::Vent => HEADLINE_VENT,
        Verdict::Open => HEADLINE_OPEN,
    };
    let mut body = format!("{location} \u{b7} PM2.5 {pm2_5:.0} \u{b5}g/m\u{b3}");
    if let Some(timing) = timing.filter(|t| !t.trim().is_empty()) {
        body.push_str(" \u{b7} ");
        body.push_str(timing);
    }
    (title.to_string(), body)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn message_leads_with_the_verdict_imperative() {
        let (title, body) = message_at(Verdict::KeepClosed, "Cairo", 26.0, None);
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
        assert_eq!(
            message_at(Verdict::Open, "Cairo", 9.0, None).0,
            HEADLINE_OPEN
        );
        assert_eq!(
            message_at(Verdict::Vent, "Cairo", 20.0, None).0,
            HEADLINE_VENT
        );
        assert_ne!(HEADLINE_OPEN, HEADLINE_VENT);
    }

    #[test]
    fn a_timed_message_keeps_the_data_prefix_and_appends_when() {
        // EXP30: the data prefix is unchanged, so pre-EXP30 assertions and the
        // user's reading habits survive; the timing is last.
        let (_, body) = message_at(
            Verdict::KeepClosed,
            "Cairo, Egypt",
            30.0,
            Some("from 4:00 PM (in 10 min)"),
        );
        assert_eq!(
            body,
            "Cairo, Egypt \u{b7} PM2.5 30 \u{b5}g/m\u{b3} \u{b7} from 4:00 PM (in 10 min)"
        );
        assert!(body.starts_with("Cairo, Egypt \u{b7} PM2.5 30 "));
        // No timing is exactly the old message.
        assert_eq!(
            message_at(Verdict::KeepClosed, "Cairo", 26.0, None).1,
            "Cairo \u{b7} PM2.5 26 \u{b5}g/m\u{b3}"
        );
        // A blank timing is no timing, not a dangling separator.
        assert_eq!(
            message_at(Verdict::KeepClosed, "Cairo", 26.0, Some("  ")),
            message_at(Verdict::KeepClosed, "Cairo", 26.0, None)
        );
    }
}
