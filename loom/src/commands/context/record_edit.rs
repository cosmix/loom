//! Compatibility entry point for the former edit-path recorder.
//!
//! Context refresh derives changes from the checkout. This command retains its
//! invocation and stage validation but no longer writes speculative artifacts.

use crate::validation::validate_id;
use anyhow::{Context, Result};
use std::path::PathBuf;

/// Validate the stage and accept legacy edit notifications without writing state.
pub fn record_edit(stage: &str, _paths: &[PathBuf]) -> Result<()> {
    validate_id(stage).context("Invalid --stage value")?;
    Ok(())
}

#[cfg(test)]
#[path = "tests_record_edit.rs"]
mod tests;
