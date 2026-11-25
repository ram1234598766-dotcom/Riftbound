//! Fixed-step simulation clock with bounded catch-up.

/// Result of advancing a fixed-step clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TickBatch {
    /// Number of fixed simulation steps to execute.
    pub steps: u32,
    /// Remaining fraction of a simulation step, in the range [0, 1).
    pub alpha: f64,
    /// Time discarded after reaching the catch-up limit.
    pub dropped_seconds: f64,
}

/// Accumulates real time into deterministic fixed-size simulation steps.
#[derive(Clone, Debug)]
pub struct FixedClock {
    step_seconds: f64,
    accumulator: f64,
    max_steps_per_advance: u32,
}

impl FixedClock {
    /// Creates a clock. `step_seconds` must be finite and positive.
    ///
    /// # Panics
    ///
    /// Panics if the step is not finite and positive, or the step cap is zero.
    #[must_use]
    pub fn new(step_seconds: f64, max_steps_per_advance: u32) -> Self {
        assert!(
            step_seconds.is_finite() && step_seconds > 0.0,
            "step must be finite and positive"
        );
        assert!(max_steps_per_advance > 0, "step cap must be nonzero");
        Self {
            step_seconds,
            accumulator: 0.0,
            max_steps_per_advance,
        }
    }

    /// Adds elapsed wall time and returns bounded simulation work for this frame.
    ///
    /// Invalid or negative elapsed values are treated as zero. If the catch-up
    /// limit is reached, excess whole steps are discarded while the fractional
    /// remainder is retained.
    pub fn advance(&mut self, elapsed_seconds: f64) -> TickBatch {
        let elapsed = if elapsed_seconds.is_finite() && elapsed_seconds > 0.0 {
            elapsed_seconds
        } else {
            0.0
        };
        let total = self.accumulator + elapsed;
        if !total.is_finite() {
            // Two finite values can still overflow when added. Preserve a valid
            // fractional remainder and report bounded work instead of poisoning
            // interpolation with infinity or NaN.
            let elapsed_remainder = elapsed % self.step_seconds;
            let combined_remainder = self.accumulator + elapsed_remainder;
            self.accumulator = combined_remainder % self.step_seconds;
            return TickBatch {
                steps: self.max_steps_per_advance,
                alpha: self.accumulator / self.step_seconds,
                dropped_seconds: elapsed - elapsed_remainder,
            };
        }
        self.accumulator = total;

        let mut steps = 0;
        while steps < self.max_steps_per_advance && self.accumulator >= self.step_seconds {
            self.accumulator -= self.step_seconds;
            steps += 1;
        }

        let mut dropped_seconds = 0.0;
        if self.accumulator >= self.step_seconds {
            let retained = self.accumulator % self.step_seconds;
            dropped_seconds = self.accumulator - retained;
            self.accumulator = retained;
        }

        TickBatch {
            steps,
            alpha: self.accumulator / self.step_seconds,
            dropped_seconds,
        }
    }

    /// Clears accumulated time, for example when simulation is paused or resumed.
    ///
    /// This prevents elapsed wall time during a pause from being simulated on the
    /// next advance. The clock keeps its configured step size and catch-up budget.
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }

    /// The fixed simulation step in seconds.
    #[must_use]
    pub const fn step_seconds(&self) -> f64 {
        self.step_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_fractional_time_and_caps_catch_up() {
        let mut clock = FixedClock::new(0.01, 3);
        let first = clock.advance(0.025);
        assert_eq!(first.steps, 2);
        assert!((first.alpha - 0.5).abs() < 1e-9);

        let second = clock.advance(1.0);
        assert_eq!(second.steps, 3);
        assert!(second.dropped_seconds > 0.9);
        assert!((0.0..1.0).contains(&second.alpha));
    }

    #[test]
    fn reset_discards_partial_and_pending_simulation_time() {
        let mut clock = FixedClock::new(0.01, 3);
        assert_eq!(clock.advance(0.025).steps, 2);

        clock.reset();

        let after_reset = clock.advance(0.005);
        assert_eq!(after_reset.steps, 0);
        assert!((after_reset.alpha - 0.5).abs() < 1e-9);
        assert_eq!(after_reset.dropped_seconds, 0.0);
    }

    #[test]
    fn ignores_invalid_elapsed_time() {
        let mut clock = FixedClock::new(1.0 / 60.0, 4);
        assert_eq!(clock.advance(f64::NAN).steps, 0);
        assert_eq!(clock.advance(-1.0).steps, 0);
    }

    #[test]
    fn finite_elapsed_overflow_keeps_interpolation_finite_and_work_bounded() {
        let mut clock = FixedClock::new(0.01, 4);
        let batch = clock.advance(f64::MAX);

        assert_eq!(batch.steps, 4);
        assert!(batch.alpha.is_finite());
        assert!((0.0..1.0).contains(&batch.alpha));
        assert!(batch.dropped_seconds.is_finite());
    }

    #[test]
    fn reports_no_dropped_time_when_within_the_step_budget() {
        let mut clock = FixedClock::new(0.25, 4);
        let batch = clock.advance(0.6);
        assert_eq!(batch.steps, 2);
        assert_eq!(batch.dropped_seconds, 0.0);
        assert!((batch.alpha - 0.4).abs() < 1e-9);
    }
}
