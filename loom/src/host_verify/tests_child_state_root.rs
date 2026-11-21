//! What of the plan's state root a stage's criteria reach from their
//! worktree, as the host child runs them under loom's real confinement: a
//! knowledge-distill stage's read the memory journals, never write them and
//! read nothing else there; every other worktree stage's reach none of it.

use std::fs;

use serial_test::serial;

use super::tests_child::{confinement_unavailable, labels_and_verdicts, Fixture, STAGE};
use super::JobState;
use crate::fs::permissions::state_root::STATE_ROOT_SECRET_FILES;
use crate::models::stage::StageType;

const JOURNAL: &str = "remembered-entry";
const SECRET: &str = "secret-value";

/// The fixture with the worktree's `.loom/work` linked to the state root as
/// loom links it, a memory journal, and every secret.
fn linked_fixture(acceptance: &[&str], stage_type: StageType) -> Fixture {
    let fx = Fixture::new(acceptance, stage_type);
    fs::create_dir_all(fx.worktree.join(".loom")).expect("create worktree .loom");
    std::os::unix::fs::symlink("../../../.loom/work", fx.worktree.join(".loom/work"))
        .expect("link the state root");
    let memory = fx.work_dir.join("memory");
    fs::create_dir_all(&memory).expect("create memory dir");
    fs::write(memory.join(format!("{STAGE}.md")), JOURNAL).expect("write journal");
    for name in STATE_ROOT_SECRET_FILES {
        fs::write(fx.work_dir.join(name), SECRET).expect("write secret");
    }
    fx
}

fn run(fx: &Fixture) -> Vec<(String, bool)> {
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    assert!(result.error.is_none(), "{result:?}");
    labels_and_verdicts(&result)
        .into_iter()
        .map(|(label, passed)| (label.to_string(), passed))
        .collect()
}

fn all_pass(count: usize) -> Vec<(String, bool)> {
    (0..count)
        .map(|index| (format!("acceptance-{index}"), true))
        .collect()
}

/// A criterion passing only while `path`, relative to the worktree, reads as
/// empty: under bubblewrap it is not mounted, under Seatbelt the read fails.
fn unreadable(path: &str) -> String {
    format!("test -z \"$(cat {path} 2>/dev/null)\"")
}

fn as_strs(commands: &[String]) -> Vec<&str> {
    commands.iter().map(String::as_str).collect()
}

/// The first criterion proves the journals are in reach; the rest pass only
/// while nothing else of the state root is, and the journals stay read-only.
#[test]
#[serial]
fn a_distill_criterion_reads_only_the_memory_journals() {
    if confinement_unavailable("a_distill_criterion_reads_only_the_memory_journals") {
        return;
    }
    let mut acceptance = vec![
        format!("test \"$(cat .loom/work/memory/{STAGE}.md)\" = {JOURNAL}"),
        format!("ls .loom/work/memory | grep -qx {STAGE}.md"),
        "! touch .loom/work/memory/planted 2>/dev/null".to_string(),
        format!("! sh -c 'printf x >> .loom/work/memory/{STAGE}.md' 2>/dev/null"),
        unreadable(".loom/work/config.toml"),
        unreadable(".loom/work/stages/*"),
    ];
    acceptance.extend(
        STATE_ROOT_SECRET_FILES
            .iter()
            .map(|name| unreadable(&format!(".loom/work/{name}"))),
    );
    let fx = linked_fixture(&as_strs(&acceptance), StageType::KnowledgeDistill);
    let records = fs::read_dir(fx.work_dir.join("stages")).expect("stage records");
    assert!(records.count() > 0, "the state root holds a stage record");

    assert_eq!(run(&fx), all_pass(acceptance.len()));
    let memory = fx.work_dir.join("memory");
    assert!(!memory.join("planted").exists());
    let journal = fs::read_to_string(memory.join(format!("{STAGE}.md"))).expect("journal");
    assert_eq!(journal, JOURNAL);
}

/// The grant comes from the daemon's state root, never from the worktree's
/// link: repointed at a directory outside the tree, the link reaches nothing.
#[test]
#[serial]
fn a_repointed_link_reaches_nothing() {
    if confinement_unavailable("a_repointed_link_reaches_nothing") {
        return;
    }
    let outside = tempfile::TempDir::new().expect("outside dir");
    let planted = outside.path().canonicalize().expect("canonical outside");
    fs::create_dir_all(planted.join("memory")).expect("create planted memory");
    fs::write(planted.join("memory/marker.md"), "marker").expect("write marker");
    let fx = linked_fixture(
        &[&unreadable(".loom/work/memory/marker.md")],
        StageType::KnowledgeDistill,
    );
    let link = fx.worktree.join(".loom/work");
    fs::remove_file(&link).expect("remove link");
    std::os::unix::fs::symlink(&planted, &link).expect("repoint link");

    assert_eq!(run(&fx), all_pass(1));
}

/// The worktree stays the stage's writable tree.
#[test]
#[serial]
fn a_distill_criterion_writes_its_worktree() {
    if confinement_unavailable("a_distill_criterion_writes_its_worktree") {
        return;
    }
    let fx = linked_fixture(
        &["mkdir -p doc && printf note > doc/note.md && rm doc/note.md"],
        StageType::KnowledgeDistill,
    );
    assert_eq!(run(&fx), all_pass(1));
    assert!(!fx.worktree.join("doc/note.md").exists());
}

#[test]
#[serial]
fn standard_and_integration_criteria_cannot_read_the_state_root() {
    if confinement_unavailable("standard_and_integration_criteria_cannot_read_the_state_root") {
        return;
    }
    let read = format!("cat .loom/work/memory/{STAGE}.md");
    let list = "ls .loom/work/memory";
    for stage_type in [StageType::Standard, StageType::IntegrationVerify] {
        let fx = linked_fixture(&[&read, list], stage_type);
        assert_eq!(
            run(&fx),
            [
                ("acceptance-0".to_string(), false),
                ("acceptance-1".to_string(), false)
            ],
            "{stage_type:?}"
        );
    }
}
