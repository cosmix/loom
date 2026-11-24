//! Tests for the plan-version-2 signal section: `v2_section.rs` and its
//! review blocks in `v2_section_review.rs`.

use super::*;
use crate::fs::memory::{
    append_entry, append_to_spool, MemoryEntry, MemoryEntryType, Receipt, ReceiptOutcome,
};
use crate::verify::contracts::test_support::{contract, write_test_freeze};
use serial_test::serial;
use std::path::PathBuf;

/// DESIGN D16 BLOCK-E, copied from the design so the renderer is pinned to it.
const BLOCK_E: &str = "**Review order (plan v2):** run `loom check` once beside the final review. One fix worker handles all findings and failed criteria; re-check failures with `loom check <stage-id> --criterion <n>`, then one re-review on the follow-up model, then complete. After the final round, code edits including formatting need re-review. Comment-only lines, blank lines, and prose markdown outside skills/, agents/, commands/, loom-hooks/, tests and fixtures need none; the base must be unchanged. Block comments and every other edit need re-review. Commits do not change it.";

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

fn v2_stage_of(stage_type: StageType) -> Stage {
    Stage {
        stage_type,
        ..v2_stage()
    }
}

fn render(stage: &Stage, work_dir: &Path) -> String {
    let mut content = String::new();
    append_v2_section(&mut content, stage, work_dir);
    content
}

/// Record a pending `suggestion` entry in `stage_id`'s journal; returns its id.
fn record_suggestion(work_dir: &Path, stage_id: &str, text: &str) -> String {
    let entry = MemoryEntry::new(MemoryEntryType::Suggestion, text.to_string());
    append_entry(work_dir, stage_id, &entry).unwrap();
    entry.id
}

/// Write `reviews/<stage_id>/carried.json` in the DESIGN D12 shape, with one
/// finding whose claim spans two lines.
fn write_carried(work_dir: &Path, stage_id: &str) {
    let dir = work_dir.join("reviews").join(stage_id);
    std::fs::create_dir_all(&dir).unwrap();
    let carried = serde_json::json!({
        "version": 1,
        "carried": [{
            "id": "origin/F-1-2",
            "origin_stage": "origin",
            "finding": {
                "severity": "major", "file": "src/a.rs", "line": 42,
                "claim": "the cache\nnever expires", "scenario": "a stale entry is served",
                "rule": null
            },
            "dispute": 3
        }]
    });
    std::fs::write(dir.join("carried.json"), carried.to_string()).unwrap();
}

#[test]
fn frozen_contracts_block_names_contracts_and_commands() {
    let temp = tempfile::tempdir().unwrap();
    write_test_freeze(temp.path(), "s1", "session-1");
    let content = render(&v2_stage(), temp.path());

    assert!(content.contains("## Frozen Contracts"));
    assert!(content.contains("| `rejects-x` | `tests/x_contract.rs` | `tests::rejects_x` |"));
    assert!(content.contains("`tests/fixtures/**`"));
    assert!(content.contains("Never edit a frozen file"));
    assert!(content.contains("loom stage contracts show s1"));
    assert!(content.contains("loom stage contracts restore s1"));
    assert!(content.contains(
        "loom stage dispute-contract s1 --contract <contract-id> [--contract <contract-id> ...] \
         --reason"
    ));
    assert!(content.contains("One filing names every contract with the same cause."));
    assert!(content.contains("A filing is refused while a frozen file in the worktree is edited"));
}

/// The exact instruction, not just the flag: a signal that names `--replacement`
/// but sends the agent to edit the frozen file in place would pass a bare grep.
/// A replacement is the only way a frozen file changes.
#[test]
fn frozen_contracts_block_teaches_the_replacement_dispute() {
    let temp = tempfile::tempdir().unwrap();
    write_test_freeze(temp.path(), "s1", "session-1");
    let content = render(&v2_stage(), temp.path());

    assert!(content.contains(
        "- Every change to a frozen file travels as a replacement. Copy the frozen file to \
         `$LOOM_SCRATCH_DIR`, edit the copy there, run the stage's formatter check on it, trial \
         it with `loom check s1 --contracts --replacement <frozen-file>=<path>` (one \
         `--replacement` for each file), and when the trial passes file ONE `loom stage \
         dispute-contract s1 --contract <contract-id> [--contract <contract-id> ...] \
         --replacement <frozen-file>=<path> --reason \"...\"` naming every affected contract, \
         with the same `--replacement` values. On accept the daemon writes it into the \
         worktree, uncommitted: stage it with `git add` and commit it. The latest review round \
         still covers it."
    ));
    // One sequence: no bullet files a replacement before its trial.
    assert!(!content.contains("formatter check on it, and file"));
    assert_eq!(content.matches("travels as a replacement").count(), 1);
}

/// The agent runs the contracts early and disputes an environment red with a
/// trial-checked replacement, filed as one dispute.
#[test]
fn frozen_contracts_block_teaches_the_early_check_and_the_trial() {
    let temp = tempfile::tempdir().unwrap();
    write_test_freeze(temp.path(), "s1", "session-1");
    let content = render(&v2_stage(), temp.path());

    assert!(content.contains(
        "- Run `loom check s1 --contracts` as soon as the contract tests compile, before the \
         rest of the work."
    ));
    assert!(content.contains(
        "A contract that fails there for a reason no implementation can change is disputed at \
         once with a replacement that checks the same scenario in a form that runs under \
         confinement, trialled and filed as above."
    ));
}

#[test]
fn unfrozen_stage_renders_no_frozen_contracts_block() {
    let temp = tempfile::tempdir().unwrap();
    let content = render(&v2_stage(), temp.path());

    assert!(
        !content.contains("## Frozen Contracts"),
        "nothing is frozen yet"
    );
    assert!(content.contains("## Review Gate"));
}

#[test]
fn review_section_names_loosening_only() {
    let temp = tempfile::tempdir().unwrap();
    let content = render(&v2_stage(), temp.path());
    let (_, review) = content.split_once("## Review Gate\n").unwrap();

    assert!(review.contains("a ratchet file loosened"));
    assert!(review.contains("lowering or removing a ratchet entry raises nothing"));
    assert!(!review.contains("ratchet file changed"));
}

#[test]
fn review_section_limits_the_ratchet_allowance_to_key_integer_ledgers() {
    let temp = tempfile::tempdir().unwrap();
    let content = render(&v2_stage(), temp.path());
    let (_, review) = content.split_once("## Review Gate\n").unwrap();

    assert!(review.contains("raises nothing in a `<key> <integer>` ledger"));
    assert!(review.contains("any change to a ratchet file in another format raises an event"));
}

#[test]
fn v1_stage_renders_no_v2_block() {
    let temp = tempfile::tempdir().unwrap();
    write_test_freeze(temp.path(), "s1", "session-1");
    write_carried(temp.path(), "s1");
    record_suggestion(temp.path(), "s0", "src/a.rs:1 name the constant");

    for stage_type in [
        StageType::Standard,
        StageType::IntegrationVerify,
        StageType::KnowledgeDistill,
    ] {
        let v1 = Stage {
            plan_version: 1,
            ..v2_stage_of(stage_type)
        };
        assert!(
            render(&v1, temp.path()).is_empty(),
            "a v1 {stage_type:?} stage never gets a v2 block"
        );
    }
}

#[test]
fn review_gate_block_carries_block_e_verbatim() {
    let temp = tempfile::tempdir().unwrap();
    for stage_type in [StageType::Standard, StageType::IntegrationVerify] {
        let content = render(&v2_stage_of(stage_type), temp.path());

        assert!(content.contains("## Review Gate"), "{stage_type:?}");
        assert!(content.contains(BLOCK_E), "{stage_type:?} lacks BLOCK-E");
        assert!(content.contains("Every finding blocks completion"));
        assert!(content.contains("`loom-code-reviewer`"));
        assert!(content.contains("`models.code_reviewer_followup_model` (sonnet by default)"));
        assert!(content.contains("ONE fix worker before ONE re-review on the follow-up model"));
        assert!(content.contains("`loom stage review status s1`"));
        assert!(
            content.contains("`loom stage dispute-findings s1 --finding <id> ... --reason ...`")
        );
        assert!(content.contains("File every dispute from one review round in one command"));
        assert!(content.contains("`loom stage dispute-integrity s1 --event <id> ... --reason ...`"));
        assert!(
            !content.contains("### Carried Findings"),
            "nothing is carried"
        );
    }
    let distill = render(&v2_stage_of(StageType::KnowledgeDistill), temp.path());
    assert!(!distill.contains("## Review Gate"));
    assert!(!distill.contains("loom-reviewer-brief"));
}

#[test]
fn review_gate_names_the_reviewer_brief_section() {
    let temp = tempfile::tempdir().unwrap();
    for stage_type in [StageType::Standard, StageType::IntegrationVerify] {
        let content = render(&v2_stage_of(stage_type), temp.path());
        assert!(
            content.contains("appends a `loom-reviewer-brief` section to every reviewer brief"),
            "{stage_type:?} lacks the reviewer-brief bullet"
        );
        assert!(content.contains("Do not paste them."), "{stage_type:?}");
    }
}

#[test]
fn review_gate_lists_carried_findings_with_ids() {
    let temp = tempfile::tempdir().unwrap();
    write_carried(temp.path(), "s1");
    let content = render(&v2_stage(), temp.path());

    assert!(content.contains("### Carried Findings"));
    assert!(content.contains("- `origin/F-1-2` (major) `src/a.rs:42`: the cache never expires\n"));
}

#[test]
fn iv_stage_lists_pending_suggestion_by_id() {
    let temp = tempfile::tempdir().unwrap();
    let pending = record_suggestion(temp.path(), "s0", "src/a.rs:10 use a BTreeMap");
    let settled = record_suggestion(temp.path(), "s0", "src/b.rs:3 rename the helper");
    let receipt = MemoryEntry::receipt(
        Receipt {
            event_id: settled.clone(),
            outcome: ReceiptOutcome::Discarded,
            target: None,
        },
        "not worth it".to_string(),
    );
    append_entry(temp.path(), "s2", &receipt).unwrap();

    let content = render(&v2_stage_of(StageType::IntegrationVerify), temp.path());

    assert!(content.contains("## Reviewer Suggestions"));
    assert!(content.contains(&format!("- `{pending}` (s0): src/a.rs:10 use a BTreeMap\n")));
    assert!(
        !content.contains(&settled),
        "a receipt in any journal settles a suggestion"
    );
    assert!(content
        .contains("`loom memory resolve <id> --outcome implemented --reason <what changed>`"));
}

/// Restores the process cwd on drop, even if the test panics:
/// `set_current_dir` is process-global.
struct CwdGuard(PathBuf);

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

/// The IV block reads the journals only. The cwd sits in stage `s0`'s
/// worktree and `s0` has a journal, so a reader that also takes the current
/// worktree's undrained spool (as `loom memory pending` does) would list the
/// spooled suggestion here.
#[test]
#[serial]
fn iv_signal_ignores_suggestions_still_in_the_spool() {
    let temp = tempfile::tempdir().unwrap();
    let worktree_root = temp.path().join(".worktrees").join("s0");
    std::fs::create_dir_all(&worktree_root).unwrap();
    let _cwd = CwdGuard(std::env::current_dir().unwrap());
    std::env::set_current_dir(&worktree_root).unwrap();

    let spooled = MemoryEntry::new(
        MemoryEntryType::Suggestion,
        "src/a.rs:1 spooled".to_string(),
    );
    append_to_spool(&worktree_root, &spooled).unwrap();
    let journaled = record_suggestion(temp.path(), "s0", "src/b.rs:2 journaled");

    let content = render(&v2_stage_of(StageType::IntegrationVerify), temp.path());

    assert!(content.contains(&format!("- `{journaled}` (s0): src/b.rs:2 journaled\n")));
    assert!(
        !content.contains(&spooled.id),
        "a suggestion still in the spool is in no journal yet"
    );
}

#[test]
fn suggestion_blocks_render_only_for_their_stage_types() {
    let temp = tempfile::tempdir().unwrap();
    let iv = render(&v2_stage_of(StageType::IntegrationVerify), temp.path());
    assert!(
        !iv.contains("## Reviewer Suggestions"),
        "nothing is pending"
    );

    record_suggestion(temp.path(), "s0", "src/a.rs:10 use a BTreeMap");
    let standard = render(&v2_stage(), temp.path());
    assert!(!standard.contains("## Reviewer Suggestions"));
    assert!(!standard.contains("## Unimplemented Suggestions"));

    let distill = render(&v2_stage_of(StageType::KnowledgeDistill), temp.path());
    assert!(distill.contains("## Unimplemented Suggestions"));
    assert!(distill.contains("`loom memory pending --group`"));
    assert!(!distill.contains("## Reviewer Suggestions"));
}
