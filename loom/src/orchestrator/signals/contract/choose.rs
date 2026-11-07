//! What the contract writer decides at the freeze, as the signal shows it: the
//! Test cell of each contract and the flags the plan leaves open.

use crate::plan::schema::ContractSpec;

use super::{table_cell, ResolvedContract};

/// What the Test column says when the plan gave no hint and no earlier freeze
/// saved a choice.
const TEST_UNCHOSEN: &str = "(you choose)";

/// The Test cell: the plan's hint, the test an earlier freeze saved (marked as
/// such), or `(you choose)`.
pub(super) fn test_cell(contract: &ResolvedContract<'_>) -> String {
    match &contract.spec.test {
        Some(test) if contract.test_saved => {
            format!("`{}` (chosen by an earlier freeze)", table_cell(test))
        }
        Some(test) => format!("`{}`", table_cell(test)),
        None => TEST_UNCHOSEN.to_string(),
    }
}

/// What the writer decides at the freeze and what the plan already fixed. A
/// `runner` or `cwd` the plan sets cannot be chosen; for the rest, the daemon
/// checks the writer's flags against the stage base before it accepts them.
pub(super) fn append_choices(content: &mut String, plan: &[ContractSpec]) {
    let fixed = |has: fn(&ContractSpec) -> bool| {
        let ids: Vec<String> = plan
            .iter()
            .filter(|spec| has(spec))
            .map(|spec| format!("`{}`", table_cell(&spec.id)))
            .collect();
        if ids.is_empty() {
            "none".to_string()
        } else {
            ids.join(", ")
        }
    };
    content.push_str(&format!(
        "\n### What you choose\n\nA test shown with no mark is the plan's hint: pass \
         `--test <id>=<identifier>` to replace it. The plan fixes `runner` for {} and `cwd` \
         for {}. For every other contract you may pass `--runner <id>=<adapter>` (a known \
         adapter) and `--cwd <id>=<dir>` (an ancestor of the file, inside the working \
         directory, holding the runner's root marker as committed at the stage base); the \
         daemon refuses a directory whose marker you added yourself.\n\n",
        fixed(|spec| spec.runner.is_some()),
        fixed(|spec| spec.cwd.is_some()),
    ));
}
