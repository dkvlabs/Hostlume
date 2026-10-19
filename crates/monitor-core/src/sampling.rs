//! Deterministic sampling math for interval-derived metrics
//! (contract §4): rates come from monotonic elapsed time, never wall
//! clocks, and counter resets/sleep gaps must surface as explicit states,
//! not invented numbers.

/// Outcome of feeding one cumulative counter sample to a [`RateTracker`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RateOutcome {
    /// First observation (or tracker was reset): no rate yet — `WARMUP`.
    Warmup,
    /// Elapsed monotonic time was zero: the sample carries no new interval;
    /// keep the previous state (`WARMUP` semantics), never divide by zero.
    NoElapsedTime,
    /// Counter went backwards: baseline reset, report `COUNTER_RESET`.
    CounterReset,
    /// Valid rate in units per second (counter delta / elapsed seconds).
    Rate(f64),
}

/// Tracks one monotonic counter and derives its rate between observations.
/// One tracker per (collector, entity). `reset()` must be called on sleep
/// resume or interface/device replacement so the next sample re-warms.
#[derive(Debug, Default, Clone)]
pub struct RateTracker {
    prev: Option<(u64, u64)>,
}

impl RateTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Forget the baseline; the next [`RateTracker::record`] is a warm-up.
    pub fn reset(&mut self) {
        self.prev = None;
    }

    /// Record a cumulative counter with the current monotonic millisecond
    /// reading and compute the rate since the previous observation.
    pub fn record(&mut self, counter: u64, monotonic_ms: u64) -> RateOutcome {
        match self.prev {
            None => {
                self.prev = Some((counter, monotonic_ms));
                RateOutcome::Warmup
            }
            Some((prev_counter, prev_ms)) => {
                let elapsed_ms = monotonic_ms.saturating_sub(prev_ms);
                if elapsed_ms == 0 {
                    // Invalid duration: keep the old baseline so a later
                    // sample still spans a real interval.
                    return RateOutcome::NoElapsedTime;
                }
                if counter < prev_counter {
                    // Reset/wrap: re-baseline and refuse to invent a rate.
                    self.prev = Some((counter, monotonic_ms));
                    return RateOutcome::CounterReset;
                }
                self.prev = Some((counter, monotonic_ms));
                RateOutcome::Rate((counter - prev_counter) as f64 / (elapsed_ms as f64 / 1_000.0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_rate_math_phase_doc_example() {
        // 1000 byte delta over 0.5 s = 2000 B/s (phase 02 §3).
        let mut t = RateTracker::new();
        assert_eq!(t.record(100, 0), RateOutcome::Warmup);
        assert_eq!(t.record(1100, 500), RateOutcome::Rate(2000.0));
    }

    #[test]
    fn fractional_rates_are_precise() {
        let mut t = RateTracker::new();
        t.record(0, 1_000);
        // 333 bytes over 0.75 s.
        assert_eq!(t.record(333, 1_750), RateOutcome::Rate(444.0));
    }

    #[test]
    fn counter_reset_is_reported_and_rebaselined() {
        let mut t = RateTracker::new();
        t.record(10_000, 0);
        assert_eq!(t.record(5, 1_000), RateOutcome::CounterReset);
        // After the reset the next delta is measured from the new baseline.
        assert_eq!(t.record(1_005, 2_000), RateOutcome::Rate(1000.0));
    }

    #[test]
    fn zero_elapsed_time_is_invalid_not_zero_rate() {
        let mut t = RateTracker::new();
        t.record(100, 1_000);
        assert_eq!(t.record(200, 1_000), RateOutcome::NoElapsedTime);
        // Baseline preserved: the next real interval spans the full delta.
        assert_eq!(t.record(1_100, 1_500), RateOutcome::Rate(2000.0));
    }

    #[test]
    fn reset_rewarm_and_32bit_wrap_semantics() {
        let mut t = RateTracker::new();
        t.record(u32::MAX as u64 - 10, 0);
        // A 32-bit counter wrapping through zero appears as a decrease.
        assert_eq!(t.record(90, 1_000), RateOutcome::CounterReset);
        t.reset();
        assert_eq!(t.record(90, 2_000), RateOutcome::Warmup);
        assert_eq!(t.record(190, 3_000), RateOutcome::Rate(100.0));
    }
}
