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

use anyhow::Result;

use crate::fs::memory::format_memory_for_handoff;
use crate::git::branch::current_branch;
use crate::git::runner::run_git;
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
    let stage = handoff.stage;
    let goals = format!(
        "{}\n\nHandoff created via: relay (trigger: {})",
        stage.description.clone().unwrap_or_default(),
        handoff.trigger
    );
    let content = HandoffContent::new(handoff.session.id.clone(), stage.id.clone())
        .with_plan_id(stage.plan_id.clone())
        .with_goals(goals)
        .with_current_branch(current_branch(handoff.checkout).ok())
        .with_files_modified(modified_files(handoff.checkout))
        .with_memory_content(format_memory_for_handoff(work_dir, &stage.id))
        .with_next_steps(handoff.message.map(str::to_string).into_iter().collect())
        .with_context_tokens(stage_context_tokens(work_dir, &stage.id).unwrap_or(0));
    if handoff.ends_turn {
        content.with_origin(HandoffOrigin::AgentCeiling)
    } else {
        content
    }
}

/// Build the document and write it into `W/handoffs/`.
pub fn write_session_handoff(work_dir: &Path, handoff: &SessionHandoff<'_>) -> Result<PathBuf> {
    let content = build_session_content(work_dir, handoff);
    generate_handoff(handoff.session, handoff.stage, content, work_dir)
}

/// Paths `git status --short` lists in `checkout`; empty when git cannot say.
fn modified_files(checkout: &Path) -> Vec<String> {
    match run_git(&["status", "--short"], checkout) {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.get(3..))
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .collect(),
        _ => Vec::new(),
    }
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
        let work = tmp.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        let stage = stage();
        let session = Session::new();

        let path =
            write_session_handoff(&work, &handoff(&session, &stage, tmp.path(), false)).unwrap();

        assert_eq!(path, work.join("handoffs").join("s1-handoff-001.md"));
        assert!(std::fs::read_to_string(path).unwrap().contains(&session.id));
    }
}
