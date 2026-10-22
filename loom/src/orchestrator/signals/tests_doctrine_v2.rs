//! Plan-version-2 doctrine pins (DESIGN D16), split out of `tests_doctrine.rs`
//! to keep that file under the line-count ceiling.
//!
//! - BLOCK-E (the review order): the review section a v2 stage's signal renders
//!   (`v2_section_review.rs`) and `skills/loom-orchestration/SKILL.md`.
//! - Every test-runner adapter is named in the language skill its language
//!   routes to, so a contract author reading that skill finds the adapter.
//! - The plan-writer skill routes v2 authors to `loom project detect` and the
//!   contracts reference.
//!
//! - BLOCK-F (when the stage cannot finish): the standard, integration-verify
//!   and knowledge-distill stable prefixes.
//!
//! BLOCK-E is defined here rather than in `tests_doctrine_blocks.rs`, as BLOCK-C
//! is in `tests_doctrine_waiting.rs`: that file is a private child of
//! `tests_doctrine`, so a sibling module cannot reach its constants.

use std::fs;
use std::path::Path;

use super::v2_section::append_v2_section;
use crate::models::stage::{Stage, StageType};
use crate::testrun::{languages, registry};

const ORCHESTRATION_SKILL: &str = include_str!("../../../../skills/loom-orchestration/SKILL.md");
const PLAN_WRITER_SKILL: &str = include_str!("../../../../skills/loom-plan-writer/SKILL.md");

/// The language skills, read at run time: which skill an adapter needs is only
/// known once `registry::all()` has been walked.
const SKILLS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../skills");

/// BLOCK-E - the plan-v2 review order, verbatim (DESIGN D16). The v2 review
/// section of a stage signal and the orchestration skill carry it byte for byte.
const BLOCK_E: &str = "**Review order (plan v2):** run `loom check` once beside the final review round; fix every finding and failed criterion, re-check the failed criteria with `loom check <stage-id> --criterion <n>` and re-review the changed files, then complete. Edit nothing after the final review round: any edit, formatting included, changes the change fingerprint and needs another round. A commit does not change it.";

/// BLOCK-F - when the stage cannot finish, verbatim. The standard,
/// integration-verify and knowledge-distill stable prefixes carry it byte for
/// byte; the orchestration skill, which those sessions load, does not repeat it.
const BLOCK_F: &str = "**When the stage cannot finish.** Fix what the stage can fix. A wrong criterion, wiring check, contract, review finding or test-integrity event gets a dispute: `loom stage dispute-criteria` (`--field` for wiring, wiring-tests, impact-tests entries), `dispute-contract`, `dispute-findings` or `dispute-integrity`. Filing ends this session; the daemon starts a fresh one with the verdict. Never revert, weaken or postpone correct work to avoid a dispute. A criterion failing only because of where completion runs (no network, unreadable git metadata, timeout, sandbox denial) is disputed, never blocked: one `loom stage dispute-criteria <stage-id> --criterion-index <n> [--criterion-index <n> ...]` filing names every such criterion. `loom stage block <stage-id> \"<what is needed and why>\"` is for a credential, a host install or a decision only a person can make; loom refuses it while the latest `loom stage complete` shows a failing criterion. Commit before filing either. Never end a turn asking the operator to act while the stage is executing.";

/// The body of the `## <heading>` section of `markdown`: everything after the
/// heading line up to the next level-2 heading or the end. `None` when no line
/// is exactly that heading.
fn section<'a>(markdown: &'a str, heading: &str) -> Option<&'a str> {
    let marker = format!("## {heading}\n");
    let start = markdown
        .match_indices(&marker)
        .map(|(index, _)| index)
        .find(|&index| index == 0 || markdown[..index].ends_with('\n'))?;
    let body = &markdown[start + marker.len()..];
    let end = body.find("\n## ").map_or(body.len(), |index| index + 1);
    Some(&body[..end])
}

#[test]
fn block_e_agrees_across_every_surface() {
    let work_dir = tempfile::tempdir().expect("create a temporary work dir");
    let stage = Stage {
        id: "doctrine".to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    };
    let mut signal = String::new();
    append_v2_section(&mut signal, &stage, work_dir.path());
    let review = section(&signal, "Review Gate")
        .expect("a v2 standard stage's signal renders a `## Review Gate` section");

    for (label, text) in [
        ("the v2 review section of a standard stage signal", review),
        ("skills/loom-orchestration/SKILL.md", ORCHESTRATION_SKILL),
    ] {
        assert!(
            text.contains(BLOCK_E),
            "{label} does not carry BLOCK-E verbatim. The plan-v2 review order must be \
             byte-identical wherever it appears; reword one copy and you must reword all \
             of them. Expected to find:\n{BLOCK_E}"
        );
    }
}

#[test]
fn block_f_agrees_across_every_surface() {
    for (label, text) in [
        (
            "the standard stable prefix",
            super::cache::generate_stable_prefix(),
        ),
        (
            "the integration-verify stable prefix",
            super::cache::generate_integration_verify_stable_prefix(),
        ),
        (
            "the knowledge-distill stable prefix",
            super::cache::generate_knowledge_distill_stable_prefix(),
        ),
    ] {
        assert!(
            text.contains(BLOCK_F),
            "{label} does not carry BLOCK-F verbatim. The stage exit doctrine must be \
             byte-identical wherever it appears; reword one copy and you must reword all \
             of them. Expected to find:\n{BLOCK_F}"
        );
    }
}

#[test]
fn every_adapter_is_named_in_its_language_skill() {
    let adapters = registry::all();
    assert!(
        !adapters.is_empty(),
        "no test-runner adapter is registered; this test would check nothing"
    );
    let mut missing = Vec::new();
    for adapter in adapters {
        let skill = languages::skill_for(adapter.language());
        let path = Path::new(SKILLS_DIR).join(&skill).join("SKILL.md");
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let named = section(&text, "Loom Test Runner Adapter")
            .is_some_and(|body| body.contains(&format!("`{}`", adapter.name())));
        if !named {
            missing.push(format!(
                "`{}` ({}) in skills/{skill}/SKILL.md",
                adapter.name(),
                adapter.language()
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "every adapter must be named, in backticks, inside the `## Loom Test Runner \
         Adapter` section of the skill its language routes to. Missing:\n  - {}",
        missing.join("\n  - ")
    );
}

#[test]
fn plan_writer_skill_names_project_detect() {
    for needle in ["loom project detect", "references/v2-contracts.md"] {
        assert!(
            PLAN_WRITER_SKILL.contains(needle),
            "skills/loom-plan-writer/SKILL.md must name `{needle}`: v2 plan authors \
             detect each package's adapter with it and write contracts from that reference"
        );
    }
}
