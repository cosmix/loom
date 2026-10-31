//! Preserved operator test location for the shared v2 verification service.

pub(crate) use crate::verify::completion::complete_verification_v2::run_v2;

#[cfg(test)]
#[path = "complete_verification_v2_tests.rs"]
mod tests;
