//! The dynamic signal section only plan-version-2 stages get (DESIGN D16).
//!
//! One function per block, each deciding for itself whether its stage gets
//! it. v1 stages get none of them, so their signals stay exactly as they were.

use std::path::Path;

use crate::models::stage::{Stage, StageType};
use crate::verify::contracts::store::{effective_contract, load_freeze};

use super::contract::table_cell;

#[path = "v2_section_review.rs"]
mod review;

/// Append every v2 block that applies to `stage`. A no-op for v1 stages.
pub(super) fn append_v2_section(content: &mut String, stage: &Stage, work_dir: &Path) {
    if stage.plan_version != 2 {
        return;
    }
    append_frozen_contracts(content, stage, work_dir);
    review::append_review_gate(content, stage, work_dir);
    review::append_reviewer_suggestions(content, stage, work_dir);
    review::append_unimplemented_suggestions(content, stage);
}

/// A standard stage's frozen contracts: what they are, that they are not to
/// be edited, and the commands that show and restore them. Rendered once the
/// contract session has frozen them, which is before any implementation
/// session starts. Each contract shows the test the freeze ran (the writer's
/// choice over the plan's hint), not the bare plan spec.
fn append_frozen_contracts(content: &mut String, stage: &Stage, work_dir: &Path) {
    if stage.stage_type != StageType::Standard || stage.contracts.is_empty() {
        return;
    }
    let Ok(Some(record)) = load_freeze(work_dir, &stage.id) else {
        return;
    };
    content.push_str("\n## Frozen Contracts\n\n");
    content.push_str(
        "A contract session wrote these tests and froze them before you started. They fail \
         now; your implementation makes them pass.\n\n",
    );
    content.push_str("| Contract | File | Test |\n| --- | --- | --- |\n");
    for contract in &stage.contracts {
        let effective = effective_contract(&record, contract);
        content.push_str(&format!(
            "| `{}` | `{}` | `{}` |\n",
            table_cell(&effective.id),
            table_cell(&effective.file),
            table_cell(effective.test.as_deref().unwrap_or("(unrecorded)")),
        ));
    }
    content.push('\n');
    if !stage.harness.is_empty() {
        let globs: Vec<String> = stage
            .harness
            .iter()
            .map(|glob| format!("`{glob}`"))
            .collect();
        content.push_str(&format!(
            "Files matching the harness globs {} were frozen with them.\n\n",
            globs.join(", ")
        ));
    }
    content.push_str(&format!(
        "- Never edit a frozen file. `loom stage complete` compares every frozen file with its \
         frozen hash and fails on any change.\n\
         - `loom stage contracts show {id}` prints the freeze record and the frozen files.\n\
         - `loom stage contracts restore {id}` copies the frozen content back into the \
         worktree (`--contract <id>` for one contract).\n\
         - `loom stage dispute-contract {id} --contract <contract-id> --reason \"...\"` files a \
         dispute when the frozen contract itself is wrong.\n",
        id = stage.id
    ));
}

#[cfg(test)]
#[path = "v2_section_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "v2_section_effective_tests.rs"]
mod effective_tests;
