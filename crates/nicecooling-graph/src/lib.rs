//! The Graph engine: the model, the validator and the evaluator of the node Graphs. The crate has
//! no I/O and no clock of its own. The app gives it the sensor values and the time.

pub mod funcs;
pub mod model;
pub mod table;
pub mod validate;
