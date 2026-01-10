//! Diagnosis module for analyzing failed stages and providing guidance.

pub mod guidance;
pub mod signal;

pub use guidance::print_failure_guidance;
pub use signal::{generate_diagnosis_signal, DiagnosisContext};
