//! Proximity-based collectible tracking for exploration objectives.

use crate::Vec3;

/// A collectible placed in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pickup {
    /// World-space center of the collectible.
    pub position: Vec3,
    /// Maximum interaction distance.
    pub radius: f32,
    collected: bool,
}

impl Pickup {
    /// Creates a pickup when its position and radius are finite and its radius is positive.
    #[must_use]
    pub fn new(position: Vec3, radius: f32) -> Option<Self> {
        let finite = position.x.is_finite()
            && position.y.is_finite()
            && position.z.is_finite()
            && radius.is_finite();
        (finite && radius > 0.0).then_some(Self {
            position,
            radius,
            collected: false,
        })
    }

    /// Whether this pickup has already been collected.
    #[must_use]
    pub const fn is_collected(self) -> bool {
        self.collected
    }
}

/// Tracks a set of one-time proximity collectibles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PickupTracker {
    pickups: Vec<Pickup>,
    collected_count: usize,
}

impl PickupTracker {
    /// Creates an empty tracker.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pickups: Vec::new(),
            collected_count: 0,
        }
    }

    /// Adds a valid pickup and returns its stable index.
    pub fn add(&mut self, pickup: Pickup) -> usize {
        let index = self.pickups.len();
        self.pickups.push(pickup);
        index
    }

    /// Collects every uncollected pickup within range when interaction is pressed.
    ///
    /// Returns the stable indices of pickups collected during this call.
    pub fn collect_nearby(&mut self, position: Vec3, interact_pressed: bool) -> Vec<usize> {
        if !interact_pressed
            || !position.x.is_finite()
            || !position.y.is_finite()
            || !position.z.is_finite()
        {
            return Vec::new();
        }

        let mut collected = Vec::new();
        for (index, pickup) in self.pickups.iter_mut().enumerate() {
            if pickup.collected {
                continue;
            }
            let dx = position.x - pickup.position.x;
            let dy = position.y - pickup.position.y;
            let dz = position.z - pickup.position.z;
            if dx * dx + dy * dy + dz * dz <= pickup.radius * pickup.radius {
                pickup.collected = true;
                collected.push(index);
            }
        }
        self.collected_count += collected.len();
        collected
    }

    /// Number of pickups configured in this tracker.
    #[must_use]
    pub fn total(&self) -> usize {
        self.pickups.len()
    }

    /// Number of pickups collected so far.
    #[must_use]
    pub fn collected(&self) -> usize {
        self.collected_count
    }

    /// Whether every configured pickup has been collected.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.collected_count == self.pickups.len()
    }
}


/// A world-space zone that completes a mission after its prerequisites are met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtractionZone {
    position: Vec3,
    radius: f32,
}

impl ExtractionZone {
    /// Creates an extraction zone with finite coordinates and a positive radius.
    #[must_use]
    pub fn new(position: Vec3, radius: f32) -> Option<Self> {
        let finite = position.x.is_finite()
            && position.y.is_finite()
            && position.z.is_finite()
            && radius.is_finite();
        (finite && radius > 0.0).then_some(Self { position, radius })
    }

    /// Whether an interaction at this position can complete the mission.
    #[must_use]
    pub fn can_extract(self, position: Vec3, interact_pressed: bool, prerequisites_met: bool) -> bool {
        if !interact_pressed || !prerequisites_met
            || !position.x.is_finite()
            || !position.y.is_finite()
            || !position.z.is_finite()
        {
            return false;
        }
        let dx = position.x - self.position.x;
        let dy = position.y - self.position.y;
        let dz = position.z - self.position.z;
        dx * dx + dy * dy + dz * dz <= self.radius * self.radius
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_requires_all_prerequisites_interaction_and_proximity() {
        let zone = ExtractionZone::new(Vec3::new(0.0, 0.0, 0.0), 2.0).unwrap();
        let player = Vec3::new(1.0, 0.0, 0.0);
        assert!(!zone.can_extract(player, true, false));
        assert!(!zone.can_extract(player, false, true));
        assert!(!zone.can_extract(Vec3::new(4.0, 0.0, 0.0), true, true));
        assert!(zone.can_extract(player, true, true));
    }

    #[test]
    fn extraction_zone_rejects_invalid_data() {
        assert!(ExtractionZone::new(Vec3::new(0.0, f32::NAN, 0.0), 1.0).is_none());
        assert!(ExtractionZone::new(Vec3::new(0.0, 0.0, 0.0), -1.0).is_none());
    }

    #[test]
    fn pickup_rejects_invalid_position_and_radius() {
        assert!(Pickup::new(Vec3::new(f32::NAN, 0.0, 0.0), 1.0).is_none());
        assert!(Pickup::new(Vec3::new(0.0, 0.0, 0.0), 0.0).is_none());
        assert!(Pickup::new(Vec3::new(0.0, 0.0, 0.0), f32::INFINITY).is_none());
    }

    #[test]
    fn collection_requires_interaction_and_proximity() {
        let mut tracker = PickupTracker::new();
        tracker.add(Pickup::new(Vec3::new(1.0, 0.0, 0.0), 1.5).unwrap());
        assert_eq!(
            tracker.collect_nearby(Vec3::new(0.0, 0.0, 0.0), false),
            Vec::<usize>::new()
        );
        assert_eq!(
            tracker.collect_nearby(Vec3::new(5.0, 0.0, 0.0), true),
            Vec::<usize>::new()
        );
        assert_eq!(tracker.collected(), 0);
        assert_eq!(
            tracker.collect_nearby(Vec3::new(0.0, 0.0, 0.0), true),
            vec![0]
        );
        assert_eq!(tracker.collected(), 1);
        assert!(tracker.is_complete());
    }

    #[test]
    fn pickups_collect_once_and_multiple_in_range() {
        let mut tracker = PickupTracker::new();
        tracker.add(Pickup::new(Vec3::new(0.0, 0.0, 0.0), 2.0).unwrap());
        tracker.add(Pickup::new(Vec3::new(1.0, 0.0, 0.0), 2.0).unwrap());
        tracker.add(Pickup::new(Vec3::new(8.0, 0.0, 0.0), 1.0).unwrap());
        assert_eq!(
            tracker.collect_nearby(Vec3::new(0.0, 0.0, 0.0), true),
            vec![0, 1]
        );
        assert_eq!(
            tracker.collect_nearby(Vec3::new(0.0, 0.0, 0.0), true),
            Vec::<usize>::new()
        );
        assert_eq!(tracker.total(), 3);
        assert_eq!(tracker.collected(), 2);
        assert!(!tracker.is_complete());
    }

    #[test]
    fn non_finite_player_position_never_collects() {
        let mut tracker = PickupTracker::new();
        tracker.add(Pickup::new(Vec3::new(0.0, 0.0, 0.0), 2.0).unwrap());
        assert_eq!(
            tracker.collect_nearby(Vec3::new(f32::NAN, 0.0, 0.0), true),
            Vec::<usize>::new()
        );
        assert_eq!(tracker.collected(), 0);
    }
}
