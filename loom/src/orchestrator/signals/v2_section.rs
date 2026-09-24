//! The dynamic signal section only plan-version-2 stages get (DESIGN D16).
//!
//! One function per block, each deciding for itself whether its stage gets
//! it. v1 stages get none of them, so their signals stay exactly as they were.

use std::path::Path;

use crate::models::stage::{Stage, StageType};
use crate::verify::contracts::store::load_freeze;

use super::contract::table_cell;

/// Append every v2 block that applies to `stage`. A no-op for v1 stages.
pub(super) fn append_v2_section(content: &mut String, stage: &Stage, work_dir: &Path) {
    if stage.plan_version != 2 {
        return;
    }
    append_frozen_contracts(content, stage, work_dir);
}

/// A standard stage's frozen contracts: what they are, that they are not to
/// be edited, and the commands that show and restore them. Rendered once the
/// contract session has frozen them, which is before any implementation
/// session starts.
fn append_frozen_contracts(content: &mut String, stage: &Stage, work_dir: &Path) {
    if stage.stage_type != StageType::Standard || stage.contracts.is_empty() {
        return;
    }
    if !matches!(load_freeze(work_dir, &stage.id), Ok(Some(_))) {
        return;
    }
    content.push_str("\n## Frozen Contracts\n\n");
    content.push_str(
        "A contract session wrote these tests and froze them before you started. They fail \
         now; your implementation makes them pass.\n\n",
    );
    content.push_str("| Contract | File | Test |\n| --- | --- | --- |\n");
    for contract in &stage.contracts {
        content.push_str(&format!(
            "| `{}` | `{}` | `{}` |\n",
            table_cell(&contract.id),
            table_cell(&contract.file),
            table_cell(&contract.test),
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
         worktree (`--contract <id>` for one contract).\n",
        id = stage.id
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::contracts::test_support::{contract, write_test_freeze};

    fn v2_stage() -> Stage {
        Stage {
            id: "s1".to_string(),
            plan_version: 2,
            stage_type: StageType::Standard,
            contracts: vec![contract()],
            harness: vec!["tests/fixtures/**".to_string()],
            ..Stage::default()
        }
    }

    #[test]
    fn frozen_contracts_block_names_contracts_and_commands() {
        let temp = tempfile::tempdir().unwrap();
        write_test_freeze(temp.path(), "s1", "session-1");
        let mut content = String::new();
        append_v2_section(&mut content, &v2_stage(), temp.path());

        assert!(content.contains("## Frozen Contracts"));
        assert!(content.contains("| `rejects-x` | `tests/x_contract.rs` | `tests::rejects_x` |"));
        assert!(content.contains("`tests/fixtures/**`"));
        assert!(content.contains("Never edit a frozen file"));
        assert!(content.contains("loom stage contracts show s1"));
        assert!(content.contains("loom stage contracts restore s1"));
    }

    #[test]
    fn v1_and_unfrozen_stages_get_no_v2_section() {
        let temp = tempfile::tempdir().unwrap();
        let mut content = String::new();
        append_v2_section(&mut content, &v2_stage(), temp.path());
        assert!(content.is_empty(), "nothing is frozen yet");

        write_test_freeze(temp.path(), "s1", "session-1");
        let v1 = Stage {
            plan_version: 1,
            ..v2_stage()
        };
        append_v2_section(&mut content, &v1, temp.path());
        assert!(content.is_empty(), "a v1 stage never gets the v2 section");
    }
}
