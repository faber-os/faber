//! Parts on a board's pins (docs/wiring.md, D57): the data and the pure logic of guided mode.
//!
//! - [`pins`]: each board's pin map (data files in `boards/`), the Pi's header too.
//! - [`parts`]: the part templates (data files in `parts/`), one per FaberLink part class.
//! - [`allocator`]: which pins a new part can go on, and what is wrong with a plan.
//! - [`plan`]: `board.toml`, read with line numbers and written keeping its comments.
//! - [`generate`]: the sketch Faber writes from a plan, and `my_code.h`.
//!
//! Where plans live, and the API, are the machine's (`machine/wiring.rs`).

pub mod allocator;
pub mod generate;
pub mod parts;
pub mod pins;
pub mod plan;

pub use allocator::{Candidate, ExternalUse, Problem, Severity, Suggestion};
pub use parts::{BuiltIn, Catalog, Template};
pub use pins::PinMap;
pub use plan::{Plan, PlannedPart};
