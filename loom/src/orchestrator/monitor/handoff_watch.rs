//! Recovering a handoff from the handoff DOCUMENT rather than the stage file.
//!
//! A worktree session runs sandboxed: it may write `.work/handoffs` but not
//! `.work/stages`. So when an agent hits its context ceiling and runs
//! `loom handoff --trigger ceiling`, the document lands and the
//! `Executing -> NeedsHandoff` transition does not. Everything downstream of
//! the status — [`super::budget_latch::needs_handoff_event`], the takedown,
//! the re-queue — is level-triggered on that status, so none of it ever armed
//! and the stage sat `Executing` behind an agent that had already stopped.
//!
//! The document is the signal the sandbox does allow, so the daemon reads it
//! directly. A document is a takedown request only when it carries
//! [`HandoffOrigin::AgentCeiling`]: precompact, session_end and manual
//! documents record context for a session that is still working, and the
//! daemon's own advisory snapshots carry their own origins. Acting on those
//! would kill a healthy agent every time it compacted.
//!
//! # Cost
//!
//! This runs on every poll tick (5 s) for every running session, so no handoff
//! document is ever parsed twice: [`HandoffWatch`] caches the answer per
//! filename and only reads names it has not examined before. Handoff artifacts
//! are allocated under a directory lock with a fresh number and written
//! atomically, never rewritten in place, so a cached answer cannot go stale.
//! Each refresh costs one `read_dir` over a directory holding a handful of
//! files; only the head of each NEW file is read, not its whole body.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

use crate::handoff::HandoffOrigin;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

use super::events::MonitorEvent;

/// How much of a document's head is read to find its frontmatter.
///
/// The identity fields are serialized first and the body that follows the
/// frontmatter can run to tens of kilobytes, so reading the whole file to
/// learn two strings is waste. A document whose frontmatter does not close
/// within this budget is not evidence of anything and is skipped.
const FRONTMATTER_SCAN_BYTES: u64 = 8 * 1024;

/// The takedown a handoff document asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TakedownRequest {
    session_id: String,
    stage_id: String,
}

/// The frontmatter fields this watch reads. Deliberately a subset of
/// `HandoffV2`: a document missing unrelated fields still names its session
/// perfectly well, and failing to parse it would lose a real request.
#[derive(Debug, Deserialize)]
struct HandoffFrontmatter {
    session_id: String,
    stage_id: String,
    #[serde(default)]
    origin: Option<HandoffOrigin>,
}

/// Per-filename memory of which documents ask for a takedown.
#[derive(Default)]
pub(super) struct HandoffWatch {
    /// Every handoff filename examined so far, mapped to the request it
    /// carries — `None` for the documents that are not takedown requests,
    /// which is most of them. Kept so an unreadable or routine document is
    /// dismissed once rather than re-read every tick.
    examined: HashMap<String, Option<TakedownRequest>>,
}

impl HandoffWatch {
    /// The `SessionNeedsHandoff` a handoff document asks for, if one does.
    ///
    /// Only for a stage still `Executing` and still naming this session: a
    /// stage already in `NeedsHandoff` belongs to the status-triggered path,
    /// and re-emitting there would duplicate its work for nothing.
    ///
    /// Re-emitting on later ticks is safe and deliberate. `on_needs_handoff`
    /// is guarded by `event_targets_current_session`, and leaving the stage in
    /// `NeedsHandoff` after a failed takedown is exactly how the existing
    /// retry works — a suppression latch here would swallow that retry.
    pub(super) fn needs_handoff_from_document(
        &mut self,
        session: &Session,
        stages: &[Stage],
        work_dir: &Path,
    ) -> Option<MonitorEvent> {
        let stage_id = session.stage_id.as_deref()?;
        if !stage_is_executing_for(stages, stage_id, &session.id) {
            return None;
        }

        self.refresh(work_dir);
        let requested = self
            .examined
            .values()
            .flatten()
            .any(|request| request.stage_id == stage_id && request.session_id == session.id);

        requested.then(|| MonitorEvent::SessionNeedsHandoff {
            session_id: session.id.clone(),
            stage_id: stage_id.to_string(),
        })
    }

    /// Examine handoff filenames not seen before. A directory that cannot be
    /// listed, or a file that cannot be read or parsed, is simply no evidence:
    /// nothing here may abort the poll.
    fn refresh(&mut self, work_dir: &Path) {
        let handoffs_dir = work_dir.join("handoffs");
        let Ok(entries) = std::fs::read_dir(&handoffs_dir) else {
            return;
        };

        for entry in entries.flatten() {
            let filename = entry.file_name().to_string_lossy().into_owned();
            if !filename.ends_with(".md") || self.examined.contains_key(&filename) {
                continue;
            }
            self.examined
                .insert(filename, takedown_request(&entry.path()));
        }
    }
}

/// Whether `stages` says this exact session is the one executing `stage_id`.
fn stage_is_executing_for(stages: &[Stage], stage_id: &str, session_id: &str) -> bool {
    stages.iter().any(|stage| {
        stage.id == stage_id
            && stage.status == StageStatus::Executing
            && stage.session.as_deref() == Some(session_id)
    })
}

/// The takedown request a document carries, or `None` for anything that is not
/// one: an unreadable file, a document without frontmatter, and every origin
/// other than an agent's own ceiling handoff.
fn takedown_request(path: &Path) -> Option<TakedownRequest> {
    let frontmatter: HandoffFrontmatter = serde_yaml::from_str(&frontmatter_head(path)?).ok()?;
    if frontmatter.origin != Some(HandoffOrigin::AgentCeiling) {
        return None;
    }
    Some(TakedownRequest {
        session_id: frontmatter.session_id,
        stage_id: frontmatter.stage_id,
    })
}

/// The YAML frontmatter at the head of a handoff document.
///
/// Reads at most [`FRONTMATTER_SCAN_BYTES`], so the markdown body a handoff
/// carries is never paid for. Lossy UTF-8 conversion is deliberate: the cut
/// can land inside a multi-byte character, and that character is always well
/// past the identity fields.
fn frontmatter_head(path: &Path) -> Option<String> {
    let mut head = Vec::new();
    File::open(path)
        .ok()?
        .take(FRONTMATTER_SCAN_BYTES)
        .read_to_end(&mut head)
        .ok()?;
    let head = String::from_utf8_lossy(&head);
    let body = head.trim_start().strip_prefix("---")?;
    let end = body.find("---")?;
    Some(body[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::session::SessionStatus;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// A stage as the executor leaves it: executing, naming its agent.
    fn executing_stage(session_id: &str) -> Stage {
        let mut stage = Stage::new("Climate".to_string(), None);
        stage.id = "climate-timezone-data".to_string();
        stage.status = StageStatus::Executing;
        stage.assign_session(session_id.to_string());
        stage
    }

    fn running_session(stage_id: &str) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session.status = SessionStatus::Running;
        session
    }

    /// A handoff document with the frontmatter shape the generator writes,
    /// followed by a body long enough that a whole-file read would be waste.
    fn write_handoff(work_dir: &Path, number: u32, stage_id: &str, session_id: &str, origin: &str) {
        let dir = work_dir.join("handoffs");
        std::fs::create_dir_all(&dir).unwrap();
        let origin_line = if origin.is_empty() {
            String::new()
        } else {
            format!("origin: {origin}\n")
        };
        let document = format!(
            "---\nversion: 2\nsession_id: {session_id}\nstage_id: {stage_id}\n\
             context_tokens: 153518\n{origin_line}branch: loom/{stage_id}\n---\n\n\
             # Handoff: {stage_id}\n\n{}",
            "Body text that the watch must never need to read.\n".repeat(400)
        );
        std::fs::write(
            dir.join(format!("{stage_id}-handoff-{number:03}.md")),
            document,
        )
        .unwrap();
    }

    fn work_dir() -> (TempDir, PathBuf) {
        let temp = TempDir::new().unwrap();
        let path = temp.path().to_path_buf();
        (temp, path)
    }

    /// The incident itself: the agent's ceiling handoff landed, the stage
    /// transition did not, and the daemon had nothing to react to.
    #[test]
    fn a_ceiling_document_asks_for_the_takedown_the_stage_file_never_recorded() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let stages = vec![executing_stage(&session.id)];
        write_handoff(
            &work,
            2,
            "climate-timezone-data",
            &session.id,
            "agent_ceiling",
        );

        let event = HandoffWatch::default().needs_handoff_from_document(&session, &stages, &work);

        assert_eq!(
            event,
            Some(MonitorEvent::SessionNeedsHandoff {
                session_id: session.id.clone(),
                stage_id: "climate-timezone-data".to_string(),
            })
        );
    }

    /// A document from a previous attempt names a session the stage has moved
    /// past. Acting on it would take down the successor now doing the work.
    #[test]
    fn a_document_from_another_session_is_not_this_sessions_request() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let stages = vec![executing_stage(&session.id)];
        write_handoff(
            &work,
            1,
            "climate-timezone-data",
            "session-from-a-previous-attempt",
            "agent_ceiling",
        );

        assert_eq!(
            HandoffWatch::default().needs_handoff_from_document(&session, &stages, &work),
            None
        );
    }

    /// A stage that reached `NeedsHandoff` is the status-triggered path's to
    /// recover; this one must not double-emit behind it.
    #[test]
    fn a_stage_already_needing_handoff_does_not_emit_again() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let mut stage = executing_stage(&session.id);
        stage.status = StageStatus::NeedsHandoff;
        write_handoff(
            &work,
            1,
            "climate-timezone-data",
            &session.id,
            "agent_ceiling",
        );

        assert_eq!(
            HandoffWatch::default().needs_handoff_from_document(&session, &[stage], &work),
            None
        );
    }

    /// The regression this origin gate exists to prevent: a session at 90%
    /// context has an advisory snapshot written for it by the daemon and is
    /// expected to keep working. Reading that as a takedown request would kill
    /// every session that ever entered the Red band, or compacted.
    #[test]
    fn routine_documents_are_not_takedown_requests() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let stages = vec![executing_stage(&session.id)];
        for (number, origin) in [(1, "red_band"), (2, "budget_exceeded"), (3, "")] {
            write_handoff(&work, number, "climate-timezone-data", &session.id, origin);
        }

        assert_eq!(
            HandoffWatch::default().needs_handoff_from_document(&session, &stages, &work),
            None
        );
    }

    /// A truncated, malformed or bodyless file is no evidence — and above all
    /// must not abort the tick for every other session.
    #[test]
    fn malformed_documents_are_skipped_silently() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let stages = vec![executing_stage(&session.id)];
        let dir = work.join("handoffs");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("climate-timezone-data-handoff-001.md"), "").unwrap();
        std::fs::write(
            dir.join("climate-timezone-data-handoff-002.md"),
            "---\nversion: 2\nsession_id: [broken\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("climate-timezone-data-handoff-003.md"),
            "# Prose handoff with no frontmatter at all\n",
        )
        .unwrap();

        assert_eq!(
            HandoffWatch::default().needs_handoff_from_document(&session, &stages, &work),
            None
        );
    }

    /// A missing handoffs directory is the normal state of a fresh plan.
    #[test]
    fn a_missing_handoffs_directory_is_not_an_error() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let stages = vec![executing_stage(&session.id)];

        assert_eq!(
            HandoffWatch::default().needs_handoff_from_document(&session, &stages, &work),
            None
        );
    }

    /// The cost control: a document is parsed once, ever. A later tick answers
    /// from the cache even if the file has since become unreadable.
    #[test]
    fn a_document_is_parsed_once_and_answered_from_cache() {
        let (_temp, work) = work_dir();
        let session = running_session("climate-timezone-data");
        let stages = vec![executing_stage(&session.id)];
        write_handoff(
            &work,
            2,
            "climate-timezone-data",
            &session.id,
            "agent_ceiling",
        );

        let mut watch = HandoffWatch::default();
        assert!(watch
            .needs_handoff_from_document(&session, &stages, &work)
            .is_some());

        std::fs::remove_file(
            work.join("handoffs")
                .join("climate-timezone-data-handoff-002.md"),
        )
        .unwrap();

        assert!(
            watch
                .needs_handoff_from_document(&session, &stages, &work)
                .is_some(),
            "the second tick must not re-read the file it already examined"
        );
    }
}
