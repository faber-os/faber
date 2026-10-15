//! Geometry, raycasting, collision, kinematics and sensors of the simulated world.

pub mod geometry;
pub mod grid;
pub mod kinematics;
pub mod map;
pub mod rng;
pub mod sensors;
pub mod sim;

pub use geometry::{Bounds, Footprint, Pose, Segment, Vec2, mount_pose, wrap_angle};
pub use grid::{Hit, SegmentGrid};
pub use kinematics::{BaseSpec, Twist, integrate};
pub use map::{Geometry, Mover, MoverState, Shape, World};
pub use rng::Rng;
pub use sensors::{DistanceCone, DistanceReading, DistanceStatus, ScanSpec, distance, scan};
pub use sim::{Motion, STEP_S, WorldSim};
