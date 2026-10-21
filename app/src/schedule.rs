//! Wall-clock alignment for the real-source refresh loop (EXP18).
//!
//! The refresh used to sleep a fixed period measured from the last fetch, so it
//! inherited the launch minute: a 12:22 launch refreshed at 12:52, 13:22, 13:52,
//! ... Instead the loop sleeps to the next wall-clock multiple of the period, so
//! it ticks at `:00` and `:30` whatever minute the app happened to start.
//!
//! Kept free of the clock (and of any global state) so the arithmetic is
//! unit-tested on every platform; `ui.rs` supplies `SystemTime::now()`.

use std::time::Duration;

/// How long to sleep from `now` (seconds since the Unix epoch) until the next
/// multiple of `period` (seconds).
///
/// When `now` is exactly on a boundary the full `period` is returned, not zero,
/// so a fetch that lands on the boundary does not immediately fetch again.
pub fn until_next_boundary(now_secs: u64, period_secs: u64) -> Duration {
    debug_assert!(period_secs > 0, "period must be positive");
    let into = now_secs % period_secs;
    let wait = if into == 0 { period_secs } else { period_secs - into };
    Duration::from_secs(wait)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A half-hour refresh period, the real app's cadence.
    const PERIOD: u64 = 30 * 60;
    /// 2026-01-01 00:00:00 UTC, a multiple of `PERIOD`.
    const BASE: u64 = 1_767_225_600;

    #[test]
    fn waits_to_the_next_boundary_after_a_launch_minute() {
        // A 12:22 launch sleeps 8 min to the next :30 boundary.
        assert_eq!(until_next_boundary(BASE + 12 * 3600 + 22 * 60, PERIOD), Duration::from_secs(8 * 60));
        // ...and a 12:47 launch sleeps 13 min to the :00 boundary.
        assert_eq!(until_next_boundary(BASE + 12 * 3600 + 47 * 60, PERIOD), Duration::from_secs(13 * 60));
    }

    #[test]
    fn an_exact_boundary_waits_a_full_period() {
        // On the boundary (not just "close"), wait the whole period so we do not
        // double-fetch at the tick we just served.
        assert_eq!(until_next_boundary(BASE, PERIOD), Duration::from_secs(PERIOD));
        assert_eq!(until_next_boundary(BASE + 13 * 3600, PERIOD), Duration::from_secs(PERIOD));
    }

    #[test]
    fn one_second_before_a_boundary_waits_one_second() {
        assert_eq!(until_next_boundary(BASE + PERIOD - 1, PERIOD), Duration::from_secs(1));
    }

    #[test]
    fn every_wait_lands_wholly_on_a_boundary() {
        // Sweep a full period: now + wait is always a multiple of the period and
        // the wait is never zero or more than the period.
        for offset in 0..PERIOD {
            let wait = until_next_boundary(BASE + offset, PERIOD).as_secs();
            assert!(wait >= 1 && wait <= PERIOD, "wait {wait} out of range");
            assert_eq!((BASE + offset + wait) % PERIOD, 0, "landed off-boundary");
        }
    }
}
