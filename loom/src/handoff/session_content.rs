//! The handoff document the daemon writes for a relayed `loom handoff`.
//!
//! A sandboxed session cannot write `W/handoffs/`, so its request carries only
//! the trigger and an optional note (`relay::HandoffRequest`), and the daemon
//! builds the document from what it can see of the session: the stage, the
//! stage's memory journal, the latest heartbeat context reading, and
//! `git status` of the session's checkout
//! (`doc/plans/PLAN-loom-state-confinement.md` section 4.3). It is written
//! through the same numbered, locked writer the CLI uses.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs::memory::format_memory_for_handoff;
use crate::git::worktree::WorktreeGit;
use crate::handoff::generator::{generate_handoff, HandoffContent};
use crate::handoff::HandoffOrigin;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::monitor::stage_context_tokens;

/// The `--trigger` that asks for the session's turn to end.
pub const CEILING_TRIGGER: &str = "ceiling";

/// One relayed handoff, resolved against the session record.
pub struct SessionHandoff<'a> {
    pub session: &'a Session,
    pub stage: &'a Stage,
    /// The session's checkout: its worktree, or the repository root.
    pub checkout: &'a Path,
    pub trigger: &'a str,
    pub message: Option<&'a str>,
    /// Whether this request may end the session's turn. Only such a document
    /// carries `HandoffOrigin::AgentCeiling`, the origin the daemon's handoff
    /// watch takes a session down over; a document-only handoff never does.
    pub ends_turn: bool,
}

/// The document's content, built from the session's state rather than from
/// anything the request claimed.
pub fn build_session_content(work_dir: &Path, handoff: &SessionHandoff<'_>) -> HandoffContent {
    match checkout_details(work_dir, handoff.checkout) {
        Ok((branch, files)) => assemble_content(work_dir, handoff, branch, Some(files)),
        Err(error) => {
            eprintln!("Cannot inspect handoff checkout: {error:#}");
            assemble_content(work_dir, handoff, None, None)
        }
    }
}

/// `files` is `None` when the checkout could not be inspected, which says
/// nothing about its working tree.
fn assemble_content(
    work_dir: &Path,
    handoff: &SessionHandoff<'_>,
    branch: Option<String>,
    files: Option<Vec<String>>,
) -> HandoffContent {
    let stage = handoff.stage;
    let goals = format!(
        "{}\n\nHandoff created via: relay (trigger: {})",
        stage.description.clone().unwrap_or_default(),
        handoff.trigger
    );
    let content = HandoffContent::new(handoff.session.id.clone(), stage.id.clone())
        .with_plan_id(stage.plan_id.clone())
        .with_goals(goals)
        .with_current_branch(branch)
        .with_memory_content(format_memory_for_handoff(work_dir, &stage.id))
        .with_next_steps(handoff.message.map(str::to_string).into_iter().collect())
        .with_context_tokens(stage_context_tokens(work_dir, &stage.id).unwrap_or(0));
    let content = match files {
        Some(files) => content.with_files_modified(files),
        None => content,
    };
    if handoff.ends_turn {
        content.with_origin(HandoffOrigin::AgentCeiling)
    } else {
        content
    }
}

/// Build the document and write it into `W/handoffs/`.
pub fn write_session_handoff(work_dir: &Path, handoff: &SessionHandoff<'_>) -> Result<PathBuf> {
    let (branch, files) = checkout_details(work_dir, handoff.checkout)?;
    let content = assemble_content(work_dir, handoff, branch, Some(files));
    generate_handoff(handoff.session, handoff.stage, content, work_dir)
}

fn checkout_details(work_dir: &Path, checkout: &Path) -> Result<(Option<String>, Vec<String>)> {
    let workspace = crate::fs::work_dir::WorkDir::new(work_dir)?;
    let root = workspace
        .repo_root()
        .context("handoff has no project root")?;
    let checkout = checkout.canonicalize()?;
    let git = if checkout == root.canonicalize()? {
        WorktreeGit::pinned_main(root)?
    } else {
        WorktreeGit::pinned(root, &checkout)?
    };
    let branch = git.run_checked(&["branch", "--show-current"])?;
    let branch = Some(
        if branch.trim().is_empty() {
            "HEAD"
        } else {
            branch.trim()
        }
        .to_string(),
    );
    let status = git.run_checked(&["status", "--short"])?;
    let files = status
        .lines()
        .filter_map(|line| line.get(3..))
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect();
    Ok((branch, files))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::memory::{append_entry, MemoryEntry, MemoryEntryType};
    use crate::orchestrator::monitor::heartbeat::{write_heartbeat, Heartbeat};

    fn stage() -> Stage {
        Stage {
            id: "s1".to_string(),
            description: Some("Build the thing".to_string()),
            ..Stage::default()
        }
    }

    fn handoff<'a>(
        session: &'a Session,
        stage: &'a Stage,
        dir: &'a Path,
        ends: bool,
    ) -> SessionHandoff<'a> {
        SessionHandoff {
            session,
            stage,
            checkout: dir,
            trigger: CEILING_TRIGGER,
            message: Some("continue with step 3"),
            ends_turn: ends,
        }
    }

    #[test]
    fn an_uninspectable_checkout_is_no_observation_of_its_files() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let stage = stage();
        let session = Session::new();
        let missing = tmp.path().join("no-such-checkout");

        let content = build_session_content(&work, &handoff(&session, &stage, &missing, false));

        assert!(!content.files_observed);
    }

    #[test]
    fn the_document_carries_the_journal_heartbeat_and_trigger() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let stage = stage();
        let mut session = Session::new();
        session.assign_to_stage("s1".to_string());
        let note = MemoryEntry::new(
            MemoryEntryType::Note,
            "the parser needs a fixture".to_string(),
        );
        append_entry(&work, "s1", &note).unwrap();
        let reading =
            Heartbeat::new("s1".to_string(), session.id.clone()).with_context_tokens(4321);
        write_heartbeat(&work, &reading).unwrap();

        let content = build_session_content(&work, &handoff(&session, &stage, tmp.path(), true));

        assert_eq!(content.session_id, session.id);
        assert_eq!(content.context_tokens, 4321);
        assert!(content
            .memory_content
            .unwrap()
            .contains("the parser needs a fixture"));
        assert!(content.goals.contains("Build the thing"));
        assert!(content.goals.contains("trigger: ceiling"));
        assert_eq!(content.next_steps, vec!["continue with step 3".to_string()]);
        assert_eq!(content.origin, Some(HandoffOrigin::AgentCeiling));
    }

    #[test]
    fn a_document_only_handoff_never_carries_the_ceiling_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = stage();
        let session = Session::new();

        let content =
            build_session_content(tmp.path(), &handoff(&session, &stage, tmp.path(), false));

        assert_eq!(content.origin, None);
    }

    #[test]
    fn the_document_is_written_through_the_numbered_handoff_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let workspace = crate::fs::work_dir::WorkDir::new(tmp.path()).unwrap();
        workspace.initialize().unwrap();
        crate::git::worktree::capture_main_identity(tmp.path(), workspace.root()).unwrap();
        let work = workspace.root().to_path_buf();
        let stage = stage();
        let session = Session::new();

        let path =
            write_session_handoff(&work, &handoff(&session, &stage, tmp.path(), false)).unwrap();

        assert_eq!(path, work.join("handoffs").join("s1-handoff-001.md"));
        assert!(std::fs::read_to_string(path).unwrap().contains(&session.id));
    }

    #[test]
    fn handoff_ignores_repointed_worktree_git_and_refuses_bad_config() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        let checkout = crate::verify::contracts::test_support::contract_worktree(&repo, "s1");
        let output = std::process::Command::new("git")
            .args(["config", "extensions.worktreeConfig", "true"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let workspace = crate::fs::work_dir::WorkDir::new(&repo).unwrap();
        workspace.adopt_existing().unwrap();
        let marker = crate::verify::contracts::test_support::plant_foreign_git_dir(&checkout);
        let session = Session::new();
        let stage = stage();
        write_session_handoff(
            workspace.root(),
            &handoff(&session, &stage, &checkout, false),
        )
        .unwrap();
        assert!(!marker.exists());
        let config = repo.join(".git/worktrees/s1/config.worktree");
        std::fs::write(config, "[filter \"hostile\"]\n clean = false\n").unwrap();
        assert!(write_session_handoff(
            workspace.root(),
            &handoff(&session, &stage, &checkout, false)
        )
        .is_err());
        assert!(!marker.exists());
        WorktreeGit::discovered(&checkout)
            .run(&["status", "--short"])
            .unwrap();
        assert!(
            marker.exists(),
            "foreign filter positive control did not run"
        );
    }
}
