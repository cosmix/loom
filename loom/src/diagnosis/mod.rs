//! Diagnosis module for analyzing failed stages and providing guidance.

pub mod signal;

pub use signal::{generate_diagnosis_signal, DiagnosisContext};
