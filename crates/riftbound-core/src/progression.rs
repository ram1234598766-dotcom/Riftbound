//! Player experience and level progression for RPG rewards.

/// Deterministic, allocation-free player progression state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerProgression {
    total_xp: u64,
    level: u32,
}

impl Default for PlayerProgression {
    fn default() -> Self {
        Self::new()
    }
}

impl PlayerProgression {
    /// Creates a level-one character with no earned experience.
    #[must_use]
    pub const fn new() -> Self {
        Self { total_xp: 0, level: 1 }
    }

    /// Restores progression from a saved lifetime XP total.
    #[must_use]
    pub fn from_total_xp(total_xp: u64) -> Self {
        let mut low = 1_u32;
        let mut high = u32::MAX;
        while low < high {
            let midpoint = low + (high - low) / 2 + 1;
            if Self::xp_for_level(midpoint) <= total_xp {
                low = midpoint;
            } else {
                high = midpoint - 1;
            }
        }
        Self { total_xp, level: low }
    }

    /// Adds experience and returns the number of levels gained.
    pub fn award_xp(&mut self, amount: u64) -> u32 {
        let previous_level = self.level;
        self.total_xp = self.total_xp.saturating_add(amount);
        while self.level < u32::MAX && self.total_xp >= Self::xp_for_level(self.level + 1) {
            self.level += 1;
        }
        self.level - previous_level
    }

    /// Total experience earned over the character's lifetime.
    #[must_use]
    pub const fn total_xp(self) -> u64 {
        self.total_xp
    }

    /// Current character level (starts at one).
    #[must_use]
    pub const fn level(self) -> u32 {
        self.level
    }

    /// Total experience required to reach the supplied level.
    ///
    /// The first level requires 100 XP; each subsequent level costs 50 more.
    #[must_use]
    pub const fn xp_for_level(level: u32) -> u64 {
        if level <= 1 {
            return 0;
        }
        let n = (level - 1) as u64;
        100_u64.saturating_mul(n)
            .saturating_add(25_u64.saturating_mul(n.saturating_mul(n.saturating_sub(1))))
    }

    /// Experience earned toward the next level.
    #[must_use]
    pub fn xp_into_level(self) -> u64 {
        self.total_xp.saturating_sub(Self::xp_for_level(self.level))
    }

    /// Experience still needed to reach the next level.
    #[must_use]
    pub fn xp_to_next_level(self) -> u64 {
        Self::xp_for_level(self.level.saturating_add(1)).saturating_sub(self.total_xp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_level_one_and_awards_experience() {
        let mut progression = PlayerProgression::new();
        assert_eq!(progression.level(), 1);
        assert_eq!(progression.award_xp(75), 0);
        assert_eq!(progression.xp_into_level(), 75);
        assert_eq!(progression.xp_to_next_level(), 25);
        assert_eq!(progression.award_xp(25), 1);
        assert_eq!(progression.level(), 2);
    }

    #[test]
    fn handles_multiple_level_ups_and_zero_awards() {
        let mut progression = PlayerProgression::new();
        assert_eq!(progression.award_xp(350), 2);
        assert_eq!(progression.level(), 3);
        assert_eq!(progression.xp_into_level(), 0);
        assert_eq!(progression.award_xp(0), 0);
    }

    #[test]
    fn experience_saturates_without_wrapping() {
        let mut progression = PlayerProgression::new();
        progression.award_xp(u64::MAX);
        assert_eq!(progression.total_xp(), u64::MAX);
        assert!(progression.level() > 1);
    }
}
