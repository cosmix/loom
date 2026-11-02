//! What each producer's git observation does to the uncommitted-file snapshots.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::{merge_session_handoff, HandoffContent};
use crate::handoff::git_handoff::{CommitInfo, GitHistory};
use crate::handoff::schema::{HandoffOrigin, HandoffV2, ParsedHandoff};
use crate::models::session::Session;
use crate::models::stage::Stage;

fn fixture() -> (TempDir, Session, Stage) {
    let temp = TempDir::new().unwrap();
    let mut session = Session::new();
    session.id = "session-snapshot".to_string();
    let mut stage = Stage::new("snapshot".to_string(), None);
    stage.id = "stage-snapshot".to_string();
    (temp, session, stage)
}

fn content(session: &Session, stage: &Stage) -> HandoffContent {
    HandoffContent::new(session.id.clone(), stage.id.clone())
}

fn parse(path: &Path) -> HandoffV2 {
    let text = fs::read_to_string(path).unwrap();
    ParsedHandoff::parse(&text).as_v2().unwrap().clone()
}

/// Merge a first handoff whose checkout showed `dirty.rs`.
fn merge_dirty_observation(temp: &TempDir, session: &Session, stage: &Stage) {
    let dirty = content(session, stage).with_files_modified(vec!["dirty.rs".to_string()]);
    merge_session_handoff(session, stage, None, dirty, temp.path()).unwrap();
}

fn assert_snapshot_kept(handoff: &HandoffV2) {
    assert_eq!(handoff.uncommitted_files, vec!["dirty.rs"]);
    assert_eq!(handoff.files_modified, vec!["dirty.rs"]);
}

#[test]
fn red_band_handoff_without_an_observation_keeps_the_prior_snapshot() {
    let (temp, session, stage) = fixture();
    merge_dirty_observation(&temp, &session, &stage);
    let origin = HandoffOrigin::RedBand;
    let red_band = content(&session, &stage)
        .with_origin(origin)
        .with_context_tokens(7);

    let (path, _) =
        merge_session_handoff(&session, &stage, Some(origin), red_band, temp.path()).unwrap();

    assert_snapshot_kept(&parse(&path));
}

/// The monitor's red-band handoff for a completed stage names the commit and
/// never looked at the working tree.
#[test]
fn a_commit_only_history_is_no_observation_of_the_working_tree() {
    let (temp, session, stage) = fixture();
    merge_dirty_observation(&temp, &session, &stage);
    let origin = HandoffOrigin::RedBand;
    let red_band = content(&session, &stage)
        .with_origin(origin)
        .with_git_history(Some(GitHistory {
            commits: vec![CommitInfo {
                hash: "a".repeat(40),
                message: "stage completed_commit".to_string(),
            }],
            ..GitHistory::default()
        }));

    let (path, _) =
        merge_session_handoff(&session, &stage, Some(origin), red_band, temp.path()).unwrap();

    assert_snapshot_kept(&parse(&path));
    assert_eq!(parse(&path).commits.len(), 1);
}

#[test]
fn a_newly_clean_file_observation_clears_the_prior_snapshot() {
    let (temp, session, stage) = fixture();
    merge_dirty_observation(&temp, &session, &stage);
    let clean = content(&session, &stage).with_files_modified(Vec::new());

    let (path, _) = merge_session_handoff(&session, &stage, None, clean, temp.path()).unwrap();

    let handoff = parse(&path);
    assert!(handoff.uncommitted_files.is_empty());
    assert!(handoff.files_modified.is_empty());
}

#[test]
fn a_newly_clean_git_history_clears_the_prior_snapshot() {
    let (temp, session, stage) = fixture();
    merge_dirty_observation(&temp, &session, &stage);
    let clean = content(&session, &stage).with_git_history(Some(GitHistory {
        branch: "loom/stage".to_string(),
        ..GitHistory::default()
    }));

    let (path, _) = merge_session_handoff(&session, &stage, None, clean, temp.path()).unwrap();

    assert!(parse(&path).uncommitted_files.is_empty());
}
