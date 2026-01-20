//! Tests for stage transitions, persistence, and serialization

use crate::models::stage::{Stage, StageStatus};

fn create_test_stage(id: &str, name: &str, status: StageStatus) -> Stage {
    let mut stage = Stage::new(name.to_string(), Some(format!("Test stage {name}")));
    stage.id = id.to_string();
    stage.status = status;
    stage
}

#[cfg(test)]
mod dependency_satisfaction;
#[cfg(test)]
mod dependency_triggers;
#[cfg(test)]
mod persistence;
#[cfg(test)]
mod serialization;
#[cfg(test)]
mod state_transitions;
