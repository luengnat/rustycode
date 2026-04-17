//! SWE-bench evaluation adapter
//!
//! Loads SWE-bench instances, runs RustyCode's Autonomous Mode system on each,
//! and produces evaluation-ready predictions.

pub mod instance;
pub mod predictor;
pub mod report;

pub use instance::SweBenchInstance;
pub use predictor::SweBenchRunner;
pub use report::Prediction;
