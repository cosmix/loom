//! Measures direct coordinator edits by stage. The intended orchestration
//! model delegates implementation, so these counts show where that boundary
//! is routinely crossed and needs either policy or tooling support.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::{Scope, Transcript};

use super::fmt::{heading, no_data};

#[derive(Debug, serde::Serialize)]
pub struct EditRequests {
    pub total_edits: usize,
    pub stages: Vec<StageEdits>,
}

#[derive(Debug, serde::Serialize)]
pub struct StageEdits {
    pub stage: String,
    pub edits: usize,
}

pub fn build(transcripts: &[Transcript]) -> EditRequests {
    let mut counts = BTreeMap::<String, usize>::new();
    for transcript in transcripts.iter().filter(|item| item.scope == Scope::Main) {
        let stage = stage_id(&transcript.project_slug);
        for request in transcript.requests() {
            let edits = request
                .tool_uses
                .iter()
                .filter(|tool| is_edit(&tool.name))
                .count();
            *counts.entry(stage.clone()).or_default() += edits;
        }
    }
    let total_edits = counts.values().sum();
    let mut stages = counts
        .into_iter()
        .filter(|(_, edits)| *edits > 0)
        .map(|(stage, edits)| StageEdits { stage, edits })
        .collect::<Vec<_>>();
    stages.sort_by(|left, right| {
        right
            .edits
            .cmp(&left.edits)
            .then_with(|| left.stage.cmp(&right.stage))
    });
    EditRequests {
        total_edits,
        stages,
    }
}

pub fn render(edits: &EditRequests) {
    heading("Coordinator edit requests");
    if edits.total_edits == 0 {
        no_data("main-transcript edit requests");
        return;
    }
    for stage in &edits.stages {
        println!("  {}: {}", stage.stage, stage.edits);
    }
}

fn stage_id(slug: &str) -> String {
    slug.rsplit_once("-worktrees-").map_or_else(
        || "(no stage)".to_owned(),
        |(_, tail)| {
            if tail.is_empty() {
                "(no stage)".to_owned()
            } else {
                tail.to_owned()
            }
        },
    )
}

fn is_edit(name: &str) -> bool {
    matches!(name, "Edit" | "Write" | "MultiEdit" | "NotebookEdit")
}
