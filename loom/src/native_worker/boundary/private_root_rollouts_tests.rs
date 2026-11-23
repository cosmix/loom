//! Reclaiming a released macOS boundary moves its Codex rollouts into the
//! host sessions tree first, never replacing a file there and never
//! following a link planted in the private tree; a failed move keeps the
//! root. Fixture files only.
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::super::private_root::reclaim_into;
use super::super::Quiescence;
use super::{created, fixture, INVOCATION};
use crate::native_worker::model::MacIdentity;
use crate::process::sandbox_probe::require_directory_owners;

const DAY: &str = "2026/11/21";
const ROLLOUT: &str = "rollout-2026-11-21T10-00-00-thread-m.jsonl";
const LINES: &str = "{\"type\":\"session_meta\"}\n";

fn identity(scratch_root: PathBuf) -> MacIdentity {
    MacIdentity {
        invocation: INVOCATION.into(),
        session: 1,
        leader_start: "0".into(),
        boot_id: "boot".into(),
        scratch_root,
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
    fs::write(path, text).expect("a file");
}

#[test]
fn reclaim_moves_each_rollout_to_its_relative_path_and_removes_the_root() {
    require_directory_owners!(
        "reclaim_moves_each_rollout_to_its_relative_path_and_removes_the_root"
    );
    let fixture = fixture();
    let private = created(&fixture);
    let sessions = private.codex_home.join("sessions");
    write(&sessions.join(DAY).join(ROLLOUT), LINES);
    write(&sessions.join(DAY).join("notes.txt"), "not a rollout\n");
    let host = fixture.host_home.join("sessions");
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        Quiescence::Released,
        Some(&host),
    );
    assert_eq!(verdict, Ok(Quiescence::Released));
    let moved = host.join(DAY).join(ROLLOUT);
    assert_eq!(fs::read_to_string(&moved).expect("moved"), LINES);
    assert!(!host.join(DAY).join("notes.txt").exists());
    assert!(!private.root.exists(), "the root is removed after the move");
}

#[test]
fn a_rollout_whose_name_is_taken_keeps_the_root_and_both_files() {
    require_directory_owners!("a_rollout_whose_name_is_taken_keeps_the_root_and_both_files");
    let fixture = fixture();
    let private = created(&fixture);
    let source = private.codex_home.join("sessions").join(DAY).join(ROLLOUT);
    write(&source, LINES);
    let host = fixture.host_home.join("sessions");
    let taken = host.join(DAY).join(ROLLOUT);
    write(&taken, "other\n");
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        Quiescence::Released,
        Some(&host),
    );
    let why = verdict.expect_err("a taken name fails the reclaim");
    assert!(why.contains("could not be moved"), "{why}");
    assert!(why.contains(ROLLOUT), "{why}");
    assert_eq!(fs::read_to_string(&taken).expect("kept"), "other\n");
    assert_eq!(fs::read_to_string(&source).expect("kept"), LINES);
}

#[test]
fn a_taken_name_with_the_same_bytes_counts_as_moved() {
    require_directory_owners!("a_taken_name_with_the_same_bytes_counts_as_moved");
    let fixture = fixture();
    let private = created(&fixture);
    write(&private.codex_home.join("sessions").join(ROLLOUT), LINES);
    let host = fixture.host_home.join("sessions");
    write(&host.join(ROLLOUT), LINES);
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        Quiescence::Released,
        Some(&host),
    );
    assert_eq!(verdict, Ok(Quiescence::Released));
    assert!(!private.root.exists());
}

#[test]
fn a_linked_rollout_is_not_followed_and_goes_with_the_root() {
    require_directory_owners!("a_linked_rollout_is_not_followed_and_goes_with_the_root");
    let fixture = fixture();
    let private = created(&fixture);
    let outside = fixture.cache.join("outside").join(ROLLOUT);
    write(&outside, LINES);
    let sessions = private.codex_home.join("sessions");
    symlink(&outside, sessions.join(ROLLOUT)).expect("a planted link");
    let host = fixture.host_home.join("sessions");
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        Quiescence::Released,
        Some(&host),
    );
    assert_eq!(verdict, Ok(Quiescence::Released));
    assert!(!host.join(ROLLOUT).exists());
    assert_eq!(fs::read_to_string(&outside).expect("untouched"), LINES);
}

#[test]
fn rollouts_without_a_host_sessions_tree_keep_the_root() {
    require_directory_owners!("rollouts_without_a_host_sessions_tree_keep_the_root");
    let fixture = fixture();
    let private = created(&fixture);
    write(&private.codex_home.join("sessions").join(ROLLOUT), LINES);
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        Quiescence::Released,
        None,
    );
    assert!(verdict.expect_err("no host tree").contains("HOME"));
    assert!(private.root.exists());
}

#[test]
fn an_unproven_verdict_moves_nothing() {
    require_directory_owners!("an_unproven_verdict_moves_nothing");
    let fixture = fixture();
    let private = created(&fixture);
    let source = private.codex_home.join("sessions").join(ROLLOUT);
    write(&source, LINES);
    let host = fixture.host_home.join("sessions");
    let unproven = Quiescence::Unverified("still running".into());
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        unproven.clone(),
        Some(&host),
    );
    assert_eq!(verdict, Ok(unproven));
    assert!(source.exists());
    assert!(!host.join(ROLLOUT).exists());
}

#[test]
fn a_rollout_with_another_hard_link_is_not_moved() {
    require_directory_owners!("a_rollout_with_another_hard_link_is_not_moved");
    let fixture = fixture();
    let private = created(&fixture);
    let outside = fixture.cache.join("outside").join(ROLLOUT);
    write(&outside, LINES);
    let sessions = private.codex_home.join("sessions");
    fs::hard_link(&outside, sessions.join(ROLLOUT)).expect("a planted hard link");
    let host = fixture.host_home.join("sessions");
    let verdict = reclaim_into(
        Some(&identity(private.root.clone())),
        Quiescence::Released,
        Some(&host),
    );
    assert_eq!(verdict, Ok(Quiescence::Released));
    assert!(
        !host.join(ROLLOUT).exists(),
        "the inode never reaches the host"
    );
    assert_eq!(fs::read_to_string(&outside).expect("untouched"), LINES);
}

#[test]
fn a_root_removal_would_refuse_is_never_read() {
    require_directory_owners!("a_root_removal_would_refuse_is_never_read");
    let fixture = fixture();
    let private = created(&fixture);
    // Laid out like a private root but outside `native/`: without the shape
    // check its rollout would move before `remove` refuses the root.
    let native = private.root.parent().expect("the native directory");
    let stray = native.with_file_name("elsewhere").join(INVOCATION);
    let home = private
        .codex_home
        .strip_prefix(&private.root)
        .expect("inside");
    let sessions = stray.join(home).join("sessions");
    write(&sessions.join(ROLLOUT), LINES);
    let host = fixture.host_home.join("sessions");
    let verdict = reclaim_into(
        Some(&identity(stray.clone())),
        Quiescence::Released,
        Some(&host),
    );
    let why = verdict.expect_err("not a native/<invocation> root");
    assert!(why.contains("could not be removed"), "{why}");
    assert!(sessions.join(ROLLOUT).exists());
    assert!(!host.join(ROLLOUT).exists());
}
