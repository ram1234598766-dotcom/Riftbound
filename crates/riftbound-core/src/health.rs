//! Small deterministic player health model with bounded damage protection.

/// Player health and a short post-hit protection window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerHealth {
    current: u8,
    maximum: u8,
    protection_remaining: f32,
}

impl Default for PlayerHealth {
    fn default() -> Self { Self::new(100) }
}

impl PlayerHealth {
    /// Creates a full health pool. A zero maximum is promoted to one.
    #[must_use]
    pub const fn new(maximum: u8) -> Self {
        let maximum = if maximum == 0 { 1 } else { maximum };
        Self { current: maximum, maximum, protection_remaining: 0.0 }
    }

    /// Current health points.
    #[must_use]
    pub const fn current(self) -> u8 {
        self.current
    }

    /// Maximum health points.
    #[must_use]
    pub const fn maximum(self) -> u8 {
        self.maximum
    }

    /// Whether the player can continue playing.
    #[must_use]
    pub const fn is_alive(self) -> bool {
        self.current > 0
    }

    /// Whether another hit is currently blocked.
    #[must_use]
    pub const fn is_protected(self) -> bool {
        self.protection_remaining > 0.0
    }

    /// Advances the protection timer; invalid time steps are ignored.
    pub fn update(&mut self, delta_seconds: f32) {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.protection_remaining =
                (self.protection_remaining - delta_seconds.min(0.25)).max(0.0);
        }
    }

    /// Applies one hit and starts protection. Returns true only when damage was applied.
    pub fn apply_damage(&mut self, amount: u8, protection_seconds: f32) -> bool {
        if amount == 0
            || !protection_seconds.is_finite()
            || protection_seconds <= 0.0
            || !self.is_alive()
            || self.is_protected()
        {
            return false;
        }
        self.current = self.current.saturating_sub(amount);
        self.protection_remaining = protection_seconds;
        true
    }

    /// Restores the player to full health and clears the protection timer.
    pub fn restore(&mut self) {
        self.current = self.maximum;
        self.protection_remaining = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_reduces_health_and_starts_protection() {
        let mut health = PlayerHealth::default();
        assert!(health.apply_damage(25, 1.0));
        assert_eq!(health.current(), 75);
        assert!(health.is_protected());
        assert!(!health.apply_damage(25, 1.0));
    }

    #[test]
    fn protection_expires_and_allows_another_hit() {
        let mut health = PlayerHealth::default();
        assert!(health.apply_damage(10, 0.5));
        health.update(0.5);
        assert!(!health.is_protected());
        assert!(health.apply_damage(10, 0.5));
        assert_eq!(health.current(), 80);
    }

    #[test]
    fn lethal_damage_clamps_to_zero_and_restore_refills() {
        let mut health = PlayerHealth::new(40);
        assert!(health.apply_damage(90, 1.0));
        assert_eq!(health.current(), 0);
        assert!(!health.is_alive());
        assert!(!health.apply_damage(1, 1.0));
        health.restore();
        assert_eq!(health.current(), 40);
        assert!(health.is_alive());
    }

    #[test]
    fn invalid_damage_and_time_do_not_change_health() {
        let mut health = PlayerHealth::default();
        assert!(!health.apply_damage(0, 1.0));
        assert!(!health.apply_damage(20, f32::NAN));
        health.update(f32::INFINITY);
        assert_eq!(health.current(), 100);
        assert!(!health.is_protected());
    }
}
