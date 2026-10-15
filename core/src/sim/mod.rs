//! The simulated world (docs/simulation.md, sections 7 and 8): a 2D, kinematic-only world for
//! simulated parts to drive in and sense. Pure: no threads, no clock, no I/O; the caller passes
//! time in.
//!
//! - [`world`]: geometry, raycasting, collision, the machine's kinematics and the sensors.
//! - [`worlds`]: the prebuilt worlds, built in as data files.

pub mod world;
