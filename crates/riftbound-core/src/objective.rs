//! Data-driven, deterministic objective tracking for secondary mission goals.

/// Supported secondary objective conditions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectiveKind {
    /// Collect a target number of items.
    Collect { target: u32 },
    /// Activate a target number of world interactions.
    Interact { target: u32 },
    /// Reach a target number of distinct locations.
    Visit { target: u32 },
    /// Complete a target number of encounter challenges.
    Defeat { target: u32 },
}

/// Progress for one optional or required objective.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectiveProgress {
    kind: ObjectiveKind,
    current: u32,
}

impl ObjectiveProgress {
    /// Creates a new objective with zero progress.
    #[must_use]
    pub const fn new(kind: ObjectiveKind) -> Self {
        Self { kind, current: 0 }
    }

    /// Adds progress, saturating at the objective's target.
    ///
    /// Returns true only on the call that completes the objective.
    pub fn advance(&mut self, amount: u32) -> bool {
        if amount == 0 || self.is_complete() {
            return false;
        }
        let target = self.target();
        self.current = self.current.saturating_add(amount).min(target);
        self.current == target
    }

    /// Current objective category.
    #[must_use]
    pub const fn kind(self) -> ObjectiveKind {
        self.kind
    }

    /// Current progress count.
    #[must_use]
    pub const fn current(self) -> u32 {
        self.current
    }

    /// Required progress count.
    #[must_use]
    pub const fn target(self) -> u32 {
        match self.kind {
            ObjectiveKind::Collect { target }
            | ObjectiveKind::Interact { target }
            | ObjectiveKind::Visit { target }
            | ObjectiveKind::Defeat { target } => target,
        }
    }

    /// Whether the objective has reached its target.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.current >= self.target()
    }

    /// Completion fraction in the inclusive range 0.0..=1.0.
    #[must_use]
    pub fn fraction(self) -> f32 {
        if self.target() == 0 {
            1.0
        } else {
            self.current as f32 / self.target() as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_advances_and_completes_once() {
        let mut objective = ObjectiveProgress::new(ObjectiveKind::Visit { target: 3 });
        assert!(!objective.advance(1));
        assert!(!objective.advance(1));
        assert!(objective.advance(1));
        assert!(!objective.advance(1));
        assert_eq!(objective.current(), 3);
        assert_eq!(objective.fraction(), 1.0);
    }

    #[test]
    fn progress_saturates_without_integer_overflow() {
        let mut objective = ObjectiveProgress::new(ObjectiveKind::Defeat { target: 4 });
        assert!(objective.advance(u32::MAX));
        assert_eq!(objective.current(), 4);
    }

    #[test]
    fn zero_target_is_already_complete_and_never_advances() {
        let mut objective = ObjectiveProgress::new(ObjectiveKind::Collect { target: 0 });
        assert!(objective.is_complete());
        assert_eq!(objective.fraction(), 1.0);
        assert!(!objective.advance(1));
    }

    #[test]
    fn zero_progress_does_not_change_objective() {
        let mut objective = ObjectiveProgress::new(ObjectiveKind::Interact { target: 2 });
        assert!(!objective.advance(0));
        assert_eq!(objective.current(), 0);
    }
}
