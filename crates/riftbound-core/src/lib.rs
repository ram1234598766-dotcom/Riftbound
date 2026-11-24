//! Platform-independent foundations for the Riftbound engine.
//!
//! Keep this crate deterministic and testable. Platform, rendering, audio, and
//! networking integrations belong in separate crates.

mod clock;
mod input;
mod math;

pub use clock::{FixedClock, TickBatch};
pub use input::{Button, InputState};
pub use math::{Vec2, Vec3};
