//! Platform-independent foundations for the Riftbound engine.
//!
//! Keep this crate deterministic and testable. Platform, rendering, audio, and
//! networking integrations belong in separate crates.

mod character;
mod checkpoint;
mod clock;
mod collision;
mod enemy;
mod entity;
mod health;
mod input;
mod math;
mod mission;
mod objective;
mod pickup;
mod scene;
mod terrain;
mod transform;
mod vehicle;

pub use character::{
    CharacterController, CharacterMovementState, MovementConfig, VerticalMotionConfig,
};
pub use checkpoint::{Checkpoint, CheckpointProgress};
pub use clock::{FixedClock, TickBatch};
pub use collision::{Aabb2, CollisionResult, CollisionWorld};
pub use enemy::{EnemyArchetype, EnemyBrain, EnemyConfig, EnemyDifficulty, EnemyEvent, EnemyState};
pub use entity::{EntityArena, EntityId};
pub use health::PlayerHealth;
pub use input::{Button, InputState};
pub use math::{Vec2, Vec3};
pub use mission::{MissionPhase, MissionProgress};
pub use objective::{ObjectiveKind, ObjectiveProgress};
pub use pickup::{ExtractionZone, Pickup, PickupTracker};
pub use scene::{MaterialId, MeshId, Scene, SceneObject};
pub use terrain::terrain_height;
pub use transform::Transform3;
pub use vehicle::{VehicleConfig, VehicleInput, VehicleState, VehicleSurface};
