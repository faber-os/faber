//! Geometry, raycasting, collision, kinematics and sensors of the simulated world.

pub mod geometry;
pub mod grid;
pub mod rng;

pub use geometry::{Bounds, Footprint, Pose, Segment, Vec2, mount_pose, wrap_angle};
pub use grid::{Hit, SegmentGrid};
pub use rng::Rng;
