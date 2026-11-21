//! Frozen contract: Codex may take bounded exploration; open-ended work stays with Claude.
//!
//! Surfaces checked, each whitespace-normalized, lowercased and stripped of `> ` quote prefixes:
//! BLOCK-B, the orchestration SKILL.md copy, the plan-writer SKILL.md, the plan-writer
//! `codex-implementers.md` reference, and the generated mixed-lane signals (native and legacy).
//!
//! Every surface must license bounded exploration ("bounded explor…", tied to "owned" files or
//! files "it owns") and keep open-ended exploration and debugging with Claude. BLOCK-B, its
//! orchestration copy, the reference and both signals enumerate the full reserved list:
//! unknown-cause debugging, failing-test convergence (or iteration on failing tests),
//! verification, review and planning. The reference and both signals also name the "navigation
//! kit". No surface may keep the old blanket reservation or hand debugging to Codex.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::generate::generate_signal_with_metrics;
use super::tests::{create_test_session, create_test_stage, create_test_worktree};
use super::tests_codex_lane::legacy_state_dir;
use crate::models::stage::{Implementer, Implementers};

const BLOCKS_SOURCE: &str = include_str!("tests_doctrine_blocks.rs");
const ORCHESTRATION: &str = include_str!("../../../../skills/loom-orchestration/SKILL.md");
const PLAN_WRITER: &str = include_str!("../../../../skills/loom-plan-writer/SKILL.md");
const CODEX_REFERENCE: &str =
    include_str!("../../../../skills/loom-plan-writer/references/codex-implementers.md");

/// Collapse whitespace, drop markdown quote markers and lowercase.
fn normalize(text: &str) -> String {
    let unquoted: Vec<&str> = text
        .lines()
        .map(|line| line.trim_start().trim_start_matches("> "))
        .collect();
    unquoted
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The BLOCK_B literal, cut from its defining source (the constant is private to a sibling module).
fn block_b() -> &'static str {
    let start = BLOCKS_SOURCE
        .find("const BLOCK_B: &str = r#\"")
        .expect("BLOCK_B defined");
    let body = &BLOCKS_SOURCE[start..];
    let open = body.find("r#\"").expect("raw string opens") + 3;
    let close = body[open..].find("\"#;").expect("raw string closes");
    &body[open..open + close]
}

/// A mixed claude+codex stage signal generated against `work_dir`'s lane configuration.
fn mixed_signal(work_dir: &Path) -> String {
    let mut stage = create_test_stage();
    stage.implementers = Implementers::new(vec![Implementer::Claude, Implementer::Codex]);
    let (path, _) = generate_signal_with_metrics(
        &create_test_session(),
        &stage,
        &create_test_worktree(),
        &[],
        None,
        None,
        work_dir,
    )
    .unwrap();
    fs::read_to_string(path).unwrap()
}

fn signals() -> (String, String) {
    let temp = TempDir::new().unwrap();
    let native_dir = temp.path().join("native");
    fs::create_dir_all(&native_dir).unwrap();
    let native = mixed_signal(&native_dir);
    let legacy = mixed_signal(&legacy_state_dir(&temp));
    (native, legacy)
}

fn assert_has(name: &str, text: &str, phrase: &str) {
    assert!(
        text.contains(phrase),
        "surface {name} lacks {phrase:?}: {text}"
    );
}

fn assert_lacks(name: &str, text: &str, phrase: &str) {
    assert!(
        !text.contains(phrase),
        "surface {name} still carries {phrase:?}"
    );
}

/// Claude's full reserved list, required on the surfaces that enumerate reserved work.
fn assert_full_reservation(name: &str, text: &str) {
    for phrase in [
        "unknown-cause debugging",
        "verification",
        "review",
        "planning",
    ] {
        assert_has(name, text, phrase);
    }
    assert!(
        text.contains("failing-test convergence") || text.contains("failing tests"),
        "surface {name} does not reserve failing-test convergence for Claude: {text}"
    );
}

/// Wording that sends all exploration (or debugging) to one lane.
const OLD_WORDING: [&str; 7] = [
    "planning, exploration, debugging and iteration on failing tests",
    "claude fits exploration,",
    "exploration, fixture-backed test convergence, ui design, and debugging license claude",
    "always does verification, review, planning, exploration, debugging",
    "codex may take debugging",
    "codex takes debugging",
    "codex may take unknown-cause debugging",
];

#[test]
fn codex_may_take_bounded_exploration_and_debugging_stays_with_claude() {
    let (native, legacy) = signals();
    // (name, text, enumerates Claude's full reserved list, names the navigation kit)
    let surfaces = [
        ("BLOCK_B", block_b(), true, false),
        ("orchestration SKILL.md", ORCHESTRATION, true, false),
        ("plan-writer SKILL.md", PLAN_WRITER, false, false),
        ("codex-implementers.md", CODEX_REFERENCE, true, true),
        ("native signal", native.as_str(), true, true),
        ("legacy signal", legacy.as_str(), true, true),
    ];
    for (name, raw, full, kit) in surfaces {
        let text = normalize(raw);
        assert_has(name, &text, "bounded explor");
        assert!(
            text.contains("owned") || text.contains("it owns"),
            "surface {name} does not tie bounded exploration to owned files: {text}"
        );
        assert_has(name, &text, "open-ended exploration");
        assert_has(name, &text, "debugging");
        if full {
            assert_full_reservation(name, &text);
        }
        if kit {
            assert_has(name, &text, "navigation kit");
        }
        for old in OLD_WORDING {
            assert_lacks(name, &text, old);
        }
    }
}
