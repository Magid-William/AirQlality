//! The evaluation clock (EXP30).
//!
//! The app used to have no clock of its own: the series, verdict, tray icon and
//! balloon were rewritten only when a fetch landed (`spawn_real`), so an event
//! at 16:00 could be announced at 16:30. This module supplies the *evaluation*
//! instant instead - "now" as the data sees it - independent of the network.
//!
//! The refresh schedule keeps real time (`schedule::until_next_boundary`), so
//! the two clocks cannot fight: one decides when to ask, this one decides what
//! "now" means between asks.
//!
//! Test seam: `AIRQLALITY_CLOCK_EPOCH=<rfc3339>` + `AIRQLALITY_CLOCK_SPEED=<n>`
//! make the evaluation clock start at that instant and advance `n` seconds per
//! real second (`n` = 120 turns a 10 min lead into 5 s), so the windowed E2E can
//! watch a transition without waiting for the wall clock. Production reads the
//! real clock; the seam never touches the fetch schedule.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use chrono::{DateTime, FixedOffset, Utc};

/// A fixed 60 s tick, aligned to `:00`: at most one minute of error on a 10 min
/// lead, and one wake per minute while the widget runs.
pub const TICK_SECS: u64 = 60;

/// The simulated clock, when the seam is set.
struct Sim {
    start: DateTime<FixedOffset>,
    at: Instant,
    speed: u32,
}

fn sim() -> Option<&'static Sim> {
    static SIM: OnceLock<Option<Sim>> = OnceLock::new();
    SIM.get_or_init(|| {
        let start = std::env::var("AIRQLALITY_CLOCK_EPOCH")
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(s.trim()).ok())?;
        let speed = std::env::var("AIRQLALITY_CLOCK_SPEED")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|s| *s > 0)
            .unwrap_or(1);
        Some(Sim {
            start,
            at: Instant::now(),
            speed,
        })
    })
    .as_ref()
}

/// The evaluation instant. A simulation keeps the offset it was started with,
/// so the notification's "at" reads in the tested location's wall clock.
pub fn now() -> DateTime<FixedOffset> {
    match sim() {
        Some(sim) => advance(sim.start, sim.at.elapsed(), sim.speed),
        None => Utc::now().fixed_offset(),
    }
}

/// Pure: `start` advanced by `elapsed` real time at `speed`x. Split out so the
/// seam's arithmetic is testable without timers.
///
/// Millisecond precision, not whole seconds: at 120x a half-second sleep is a
/// simulated minute, and truncating the fraction would freeze the clock between
/// whole real seconds (and make every tick a minute late).
pub fn advance(
    start: DateTime<FixedOffset>,
    elapsed: Duration,
    speed: u32,
) -> DateTime<FixedOffset> {
    let millis = (elapsed.as_millis() as i64).saturating_mul(speed.max(1) as i64);
    start + chrono::Duration::milliseconds(millis)
}

/// How fast evaluation time runs (1 in production, `n` under the seam).
pub fn speed() -> u32 {
    sim().map(|sim| sim.speed).unwrap_or(1)
}

/// Real time to sleep before the next evaluation tick: the next whole minute of
/// `now`, divided by [`speed`] so a simulated minute stays a simulated minute.
///
/// Exactly on the minute waits a full minute, not zero, so a tick that lands on
/// the boundary does not spin.
pub fn until_next_tick(now: DateTime<FixedOffset>, speed: u32) -> Duration {
    let into = now.timestamp().rem_euclid(TICK_SECS as i64) as u64;
    let sub_ms = now.timestamp_subsec_millis() as u64;
    let wait_ms = (TICK_SECS - into) * 1000 - sub_ms;
    let scaled = wait_ms / u64::from(speed.max(1));
    Duration::from_millis(scaled.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32, s: u32) -> DateTime<FixedOffset> {
        FixedOffset::east_opt(2 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 1, h, m, s)
            .single()
            .unwrap()
    }

    #[test]
    fn the_tick_lands_on_the_next_whole_minute() {
        assert_eq!(until_next_tick(at(15, 45, 0), 1), Duration::from_secs(60));
        assert_eq!(until_next_tick(at(15, 45, 1), 1), Duration::from_secs(59));
        assert_eq!(until_next_tick(at(15, 45, 59), 1), Duration::from_secs(1));
        // Sub-second precision is kept: 59.5 s in is half a second to go.
        let t = at(15, 45, 59) + chrono::Duration::milliseconds(500);
        assert_eq!(until_next_tick(t, 1), Duration::from_millis(500));
    }

    #[test]
    fn a_simulated_tick_is_scaled_to_real_time() {
        // At 120x a whole simulated minute is half a real second.
        assert_eq!(until_next_tick(at(15, 45, 0), 120), Duration::from_millis(500));
        // ... and a tick is never zero, so the loop cannot spin.
        assert!(until_next_tick(at(15, 45, 59), 120) >= Duration::from_millis(1));
    }

    #[test]
    fn advancing_a_simulated_clock_multiplies_elapsed_time() {
        let start = at(15, 45, 0);
        assert_eq!(
            advance(start, Duration::from_secs(5), 120),
            at(15, 55, 0),
            "5 real seconds at 120x is 10 simulated minutes"
        );
        assert_eq!(
            advance(start, Duration::from_secs(3), 1),
            at(15, 45, 3),
            "speed 1 is the real clock"
        );
    }
}
