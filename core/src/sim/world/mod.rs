//! Geometry, raycasting, collision, kinematics and sensors of the simulated world.

pub mod geometry;

pub use geometry::{Bounds, Footprint, Pose, Segment, Vec2, mount_pose, wrap_angle};
