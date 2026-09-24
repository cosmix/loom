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
const BLOCK_E: &str = "**Review order (plan v2):** fix every finding, run the full gate, then run the final review round, then complete. Edit nothing after the final review round: any edit, formatting included, changes the change fingerprint and needs another round. A commit does not change it.";

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
        assert!(content.contains("`loom stage review status s1`"));
        assert!(
            !content.contains("### Carried Findings"),
            "nothing is carried"
        );
    }
    let distill = render(&v2_stage_of(StageType::KnowledgeDistill), temp.path());
    assert!(!distill.contains("## Review Gate"));
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
