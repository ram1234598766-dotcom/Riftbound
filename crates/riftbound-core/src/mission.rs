//! Deterministic mission objective progression shared by game runtimes.

/// Current phase of a mission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionPhase {
    /// Recover the required collectibles.
    RecoverShards,
    /// All collectibles are recovered; reach the extraction point.
    Extract,
    /// The extraction interaction has completed the mission.
    Complete,
}

/// Small deterministic state machine for a collectible-and-extraction mission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MissionProgress {
    total_shards: usize,
    collected_shards: usize,
    phase: MissionPhase,
}

impl Default for MissionProgress {
    fn default() -> Self {
        Self::new(0)
    }
}

impl MissionProgress {
    /// Creates a mission requiring the supplied number of collectibles.
    #[must_use]
    pub const fn new(total_shards: usize) -> Self {
        Self {
            total_shards,
            collected_shards: 0,
            phase: if total_shards == 0 {
                MissionPhase::Extract
            } else {
                MissionPhase::RecoverShards
            },
        }
    }

    /// Records newly collected shards, clamps to the requirement, and advances the phase.
    ///
    /// Returns true only when this call unlocks extraction.
    pub fn record_collected(&mut self, count: usize) -> bool {
        if self.phase != MissionPhase::RecoverShards || count == 0 {
            return false;
        }
        self.collected_shards = self
            .collected_shards
            .saturating_add(count)
            .min(self.total_shards);
        if self.collected_shards == self.total_shards {
            self.phase = MissionPhase::Extract;
            true
        } else {
            false
        }
    }

    /// Completes the mission only when interaction occurs inside the extraction zone.
    ///
    /// Returns true only on the transition to Complete.
    pub fn try_extract(&mut self, interact_pressed: bool, inside_zone: bool) -> bool {
        if self.phase != MissionPhase::Extract || !interact_pressed || !inside_zone {
            return false;
        }
        self.phase = MissionPhase::Complete;
        true
    }

    /// Current mission phase.
    #[must_use]
    pub const fn phase(self) -> MissionPhase {
        self.phase
    }

    /// Number of collectibles recovered so far.
    #[must_use]
    pub const fn collected(self) -> usize {
        self.collected_shards
    }

    /// Number of collectibles required by this mission.
    #[must_use]
    pub const fn total(self) -> usize {
        self.total_shards
    }

    /// Whether all collectibles have been recovered.
    #[must_use]
    pub const fn extraction_unlocked(self) -> bool {
        matches!(self.phase, MissionPhase::Extract | MissionPhase::Complete)
    }

    /// Whether the mission has been completed.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        matches!(self.phase, MissionPhase::Complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mission_advances_only_after_all_shards_are_recorded() {
        let mut mission = MissionProgress::new(3);
        assert_eq!(mission.phase(), MissionPhase::RecoverShards);
        assert!(!mission.record_collected(1));
        assert_eq!(mission.collected(), 1);
        assert!(!mission.extraction_unlocked());
        assert!(mission.record_collected(2));
        assert_eq!(mission.phase(), MissionPhase::Extract);
        assert_eq!(mission.collected(), 3);
    }

    #[test]
    fn mission_never_overcounts_or_unlocks_twice() {
        let mut mission = MissionProgress::new(2);
        assert!(mission.record_collected(9));
        assert_eq!(mission.collected(), 2);
        assert!(!mission.record_collected(1));
        assert_eq!(mission.collected(), 2);
    }

    #[test]
    fn extraction_requires_unlocked_phase_interaction_and_zone() {
        let mut mission = MissionProgress::new(1);
        assert!(!mission.try_extract(true, true));
        mission.record_collected(1);
        assert!(!mission.try_extract(false, true));
        assert!(!mission.try_extract(true, false));
        assert!(mission.try_extract(true, true));
        assert!(mission.is_complete());
        assert!(!mission.try_extract(true, true));
    }

    #[test]
    fn zero_collectible_mission_starts_at_extraction() {
        let mut mission = MissionProgress::new(0);
        assert_eq!(mission.phase(), MissionPhase::Extract);
        assert!(!mission.record_collected(1));
        assert!(mission.try_extract(true, true));
    }
}
