//! Platform-independent foundations for the Riftbound engine.
//!
//! Keep this crate deterministic and testable. Platform, rendering, audio, and
//! networking integrations belong in separate crates.

mod clock;
mod entity;
mod input;
mod math;
mod scene;
mod transform;

pub use clock::{FixedClock, TickBatch};
pub use entity::{EntityArena, EntityId};
pub use input::{Button, InputState};
pub use math::{Vec2, Vec3};
pub use scene::{Scene, SceneObject};
pub use transform::Transform3;
