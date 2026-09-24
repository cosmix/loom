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
const BLOCK_E: &str = "**Review order (plan v2):** fix every finding, run the full gate, then run the final review round, then complete. Edit nothing after the final review round: any edit, formatting included, changes the change fingerprint and needs another round. A commit does not change it.";

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
