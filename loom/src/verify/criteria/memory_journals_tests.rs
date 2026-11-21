//! Which stages get the memory journals, and at which paths.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{memory_journals_grant, MemoryJournals};
use crate::models::stage::{Stage, StageType};

fn stage(stage_type: StageType) -> Stage {
    Stage {
        id: "distill".to_owned(),
        stage_type,
        ..Stage::default()
    }
}

/// A canonical temporary directory holding a main checkout at `main`, with a
/// `.git`.
fn main_checkout() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let main = tmp.path().canonicalize().unwrap().join("main");
    fs::create_dir_all(main.join(".git")).unwrap();
    fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    (tmp, main)
}

/// A state root at `state` with a `config.toml` and, when `journals`, a
/// `memory` directory.
fn state_root(state: &Path, journals: bool) {
    fs::create_dir_all(state).unwrap();
    fs::write(state.join("config.toml"), "[plan]\n").unwrap();
    if journals {
        fs::create_dir_all(state.join("memory")).unwrap();
    }
}

fn distill_grant(work_dir: &Path) -> Option<MemoryJournals> {
    memory_journals_grant(&stage(StageType::KnowledgeDistill), work_dir).unwrap()
}

#[test]
fn only_a_knowledge_distill_stage_gets_the_journals() {
    let (_tmp, main) = main_checkout();
    let work_dir = main.join(".loom/work");
    state_root(&work_dir, true);
    for stage_type in [
        StageType::Standard,
        StageType::Knowledge,
        StageType::IntegrationVerify,
    ] {
        let grant = memory_journals_grant(&stage(stage_type), &work_dir).unwrap();
        assert_eq!(grant, None, "{stage_type:?}");
    }
    let memory = work_dir.join("memory");
    assert_eq!(
        distill_grant(&work_dir),
        Some(MemoryJournals {
            dir: memory.clone(),
            linked: memory,
        })
    );
}

#[test]
fn a_state_root_without_journals_grants_nothing() {
    let (_tmp, main) = main_checkout();
    let work_dir = main.join(".loom/work");
    state_root(&work_dir, false);

    assert_eq!(distill_grant(&work_dir), None);
}

/// With the checkout's `.loom` a symlink, the journals' canonical path lies
/// elsewhere; the worktree's link `../../../.loom/work` still reaches them
/// through `<main>/.loom/work`, so that is where they are also mounted.
#[test]
fn a_symlinked_state_root_spelling_is_granted_where_the_link_reaches_it() {
    let (tmp, main) = main_checkout();
    let elsewhere = tmp.path().canonicalize().unwrap().join("elsewhere");
    state_root(&elsewhere.join("work"), true);
    std::os::unix::fs::symlink(&elsewhere, main.join(".loom")).unwrap();
    let work_dir = main.join(".loom/work");

    assert_eq!(
        distill_grant(&work_dir),
        Some(MemoryJournals {
            dir: elsewhere.join("work/memory"),
            linked: work_dir.join("memory"),
        })
    );
}
