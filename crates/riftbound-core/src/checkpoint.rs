//! Validated checkpoint locations and deterministic respawn selection.

use crate::Vec3;

/// A checkpoint activation zone and its safe respawn position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Checkpoint {
    position: Vec3,
    radius: f32,
}

impl Checkpoint {
    /// Creates a checkpoint with finite coordinates and a positive activation radius.
    #[must_use]
    pub fn new(position: Vec3, radius: f32) -> Option<Self> {
        let finite = position.x.is_finite()
            && position.y.is_finite()
            && position.z.is_finite()
            && radius.is_finite();
        finite
            .then_some(Self { position, radius })
            .filter(|_| radius > 0.0)
    }

    /// Safe world-space respawn position.
    #[must_use]
    pub const fn position(self) -> Vec3 {
        self.position
    }

    /// Whether an interaction at the supplied position activates this checkpoint.
    #[must_use]
    pub fn can_activate(self, player_position: Vec3, interact_pressed: bool) -> bool {
        if !interact_pressed
            || !player_position.x.is_finite()
            || !player_position.y.is_finite()
            || !player_position.z.is_finite()
        {
            return false;
        }
        let dx = player_position.x - self.position.x;
        let dy = player_position.y - self.position.y;
        let dz = player_position.z - self.position.z;
        dx * dx + dy * dy + dz * dz <= self.radius * self.radius
    }
}

/// Stores the latest activated checkpoint, falling back to the mission start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckpointProgress {
    mission_start: Vec3,
    active: Option<Checkpoint>,
}

impl CheckpointProgress {
    /// Creates checkpoint progress with a validated initial spawn position.
    #[must_use]
    pub fn new(mission_start: Vec3) -> Option<Self> {
        (mission_start.x.is_finite() && mission_start.y.is_finite() && mission_start.z.is_finite())
            .then_some(Self {
                mission_start,
                active: None,
            })
    }

    /// Activates a checkpoint only when the player interacts inside its zone.
    ///
    /// Returns true only when a new checkpoint becomes active.
    pub fn activate(
        &mut self,
        checkpoint: Checkpoint,
        player_position: Vec3,
        interact_pressed: bool,
    ) -> bool {
        if !checkpoint.can_activate(player_position, interact_pressed)
            || self.active == Some(checkpoint)
        {
            return false;
        }
        self.active = Some(checkpoint);
        true
    }

    /// Current safe respawn location.
    #[must_use]
    pub const fn respawn_position(self) -> Vec3 {
        match self.active {
            Some(checkpoint) => checkpoint.position(),
            None => self.mission_start,
        }
    }

    /// Whether a checkpoint has been activated.
    #[must_use]
    pub const fn has_active_checkpoint(self) -> bool {
        self.active.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_requires_interaction_and_proximity() {
        let checkpoint = Checkpoint::new(Vec3::new(2.0, 0.5, 3.0), 1.5).unwrap();
        assert!(!checkpoint.can_activate(Vec3::new(2.0, 0.5, 3.0), false));
        assert!(!checkpoint.can_activate(Vec3::new(8.0, 0.5, 3.0), true));
        assert!(checkpoint.can_activate(Vec3::new(2.5, 0.5, 3.0), true));
    }

    #[test]
    fn respawn_uses_mission_start_until_checkpoint_is_activated() {
        let start = Vec3::new(0.0, 0.2, -4.0);
        let checkpoint = Checkpoint::new(Vec3::new(0.0, 0.7, 2.0), 1.5).unwrap();
        let mut progress = CheckpointProgress::new(start).unwrap();
        assert_eq!(progress.respawn_position(), start);
        assert!(!progress.activate(checkpoint, start, true));
        assert!(progress.activate(checkpoint, checkpoint.position(), true));
        assert_eq!(progress.respawn_position(), checkpoint.position());
        assert!(progress.has_active_checkpoint());
    }

    #[test]
    fn checkpoint_cannot_be_reactivated_or_activated_without_interaction() {
        let checkpoint = Checkpoint::new(Vec3::new(1.0, 0.5, 1.0), 1.0).unwrap();
        let mut progress = CheckpointProgress::new(Vec3::new(0.0, 0.2, 0.0)).unwrap();
        assert!(!progress.activate(checkpoint, checkpoint.position(), false));
        assert!(progress.activate(checkpoint, checkpoint.position(), true));
        assert!(!progress.activate(checkpoint, checkpoint.position(), true));
    }

    #[test]
    fn checkpoint_rejects_invalid_configuration() {
        assert!(Checkpoint::new(Vec3::new(f32::NAN, 0.0, 0.0), 1.0).is_none());
        assert!(Checkpoint::new(Vec3::new(0.0, 0.0, 0.0), 0.0).is_none());
        assert!(CheckpointProgress::new(Vec3::new(0.0, f32::INFINITY, 0.0)).is_none());
    }
}
