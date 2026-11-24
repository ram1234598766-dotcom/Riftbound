//! Platform-independent foundations for the Riftbound engine.
//!
//! Keep this crate deterministic and testable. Platform, rendering, audio, and
//! networking integrations belong in separate crates.

mod character;
mod clock;
mod collision;
mod entity;
mod input;
mod math;
mod scene;
mod transform;

pub use character::{CharacterController, CharacterMovementState, MovementConfig, VerticalMotionConfig};
pub use clock::{FixedClock, TickBatch};
pub use collision::{Aabb2, CollisionResult, CollisionWorld};
pub use entity::{EntityArena, EntityId};
pub use input::{Button, InputState};
pub use math::{Vec2, Vec3};
pub use scene::{MaterialId, MeshId, Scene, SceneObject};
pub use transform::Transform3;
