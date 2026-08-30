//! Durable lookup for cause-specific handoff artifacts.

use std::cmp::Reverse;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::handoff::{HandoffOrigin, HandoffV2, ParsedHandoff};

use super::numbering::find_latest_handoff;

/// Find the newest valid handoff for one exact stage/session/origin tuple.
///
/// Search every numbered artifact, rather than only the latest one: a newer
/// advisory, manual, or malformed handoff must not hide the durable record of
/// a budget action. Directory and read failures are uncertainty and propagate;
/// malformed or legacy content is simply not evidence for this origin.
pub fn find_matching_handoff(
    stage_id: &str,
    session_id: &str,
    origin: HandoffOrigin,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    find_handoff_where(stage_id, work_dir, |handoff| {
        handoff.stage_id == stage_id
            && handoff.session_id == session_id
            && handoff.origin == Some(origin)
    })
}

/// Find the newest valid V2 handoff written by one stage/session pair.
///
/// Continuation uses this instead of the numerically latest file so a newer
/// malformed or wrong-session artifact cannot become the successor's context.
pub fn find_latest_session_handoff(
    stage_id: &str,
    session_id: &str,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    find_handoff_where(stage_id, work_dir, |handoff| {
        handoff.stage_id == stage_id && handoff.session_id == session_id
    })
}

/// Select the artifact a continuation may consume.
///
/// Once a stage has an outgoing session, only a valid V2 handoff from that
/// exact session is eligible. Stages with no predecessor retain the legacy
/// latest-file behavior for backwards-compatible manually seeded context.
pub fn find_continuation_handoff(
    stage_id: &str,
    outgoing_session_id: Option<&str>,
    work_dir: &Path,
) -> Result<Option<PathBuf>> {
    match outgoing_session_id {
        Some(session_id) => find_latest_session_handoff(stage_id, session_id, work_dir),
        None => find_latest_handoff(stage_id, work_dir),
    }
}

/// Return the filename stem embedded in a continuation signal.
pub fn find_continuation_handoff_name(
    stage_id: &str,
    outgoing_session_id: Option<&str>,
    work_dir: &Path,
) -> Result<Option<String>> {
    Ok(
        find_continuation_handoff(stage_id, outgoing_session_id, work_dir)?.and_then(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str().map(str::to_owned))
        }),
    )
}

fn find_handoff_where(
    stage_id: &str,
    work_dir: &Path,
    matches: impl Fn(&HandoffV2) -> bool,
) -> Result<Option<PathBuf>> {
    for path in numbered_handoff_paths(stage_id, work_dir)? {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read handoff file: {}", path.display()))?;
        let parsed = ParsedHandoff::parse(&content);
        let Some(handoff) = parsed.as_v2() else {
            continue;
        };
        if matches(handoff) {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn numbered_handoff_paths(stage_id: &str, work_dir: &Path) -> Result<Vec<PathBuf>> {
    let handoffs_dir = work_dir.join("handoffs");
    let entries = match fs::read_dir(&handoffs_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "Failed to read handoffs directory: {}",
                    handoffs_dir.display()
                )
            });
        }
    };

    let prefix = format!("{stage_id}-handoff-");
    let mut candidates = Vec::new();
    for entry in entries {
        let entry = entry.context("Failed to read handoff directory entry")?;
        let filename = entry.file_name();
        let filename = filename.to_string_lossy();
        let Some(number) = filename
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(".md"))
            .and_then(|number| number.parse::<u32>().ok())
        else {
            continue;
        };
        candidates.push((number, entry.path()));
    }
    candidates.sort_unstable_by_key(|(number, _)| Reverse(*number));
    Ok(candidates.into_iter().map(|(_, path)| path).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::HandoffV2;

    fn write_handoff(path: &Path, handoff: &HandoffV2) {
        fs::write(path, format!("---\n{}---\n", handoff.to_yaml().unwrap())).unwrap();
    }

    #[test]
    fn finds_an_older_budget_handoff_behind_newer_nonmatches() {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path().join("handoffs");
        fs::create_dir_all(&dir).unwrap();
        let budget =
            HandoffV2::new("session-1", "stage-1").with_origin(HandoffOrigin::BudgetExceeded);
        write_handoff(&dir.join("stage-1-handoff-001.md"), &budget);
        let red = HandoffV2::new("session-1", "stage-1").with_origin(HandoffOrigin::RedBand);
        write_handoff(&dir.join("stage-1-handoff-002.md"), &red);
        fs::write(dir.join("stage-1-handoff-003.md"), "malformed").unwrap();
        let other =
            HandoffV2::new("session-2", "stage-1").with_origin(HandoffOrigin::BudgetExceeded);
        write_handoff(&dir.join("stage-1-handoff-004.md"), &other);
        let wrong_stage =
            HandoffV2::new("session-1", "stage-2").with_origin(HandoffOrigin::BudgetExceeded);
        write_handoff(&dir.join("stage-1-handoff-005.md"), &wrong_stage);

        let found = find_matching_handoff(
            "stage-1",
            "session-1",
            HandoffOrigin::BudgetExceeded,
            temp.path(),
        )
        .unwrap()
        .unwrap();

        assert!(found.ends_with("stage-1-handoff-001.md"));
        let continuation = find_latest_session_handoff("stage-1", "session-1", temp.path())
            .unwrap()
            .unwrap();
        assert!(continuation.ends_with("stage-1-handoff-002.md"));
    }

    #[test]
    fn a_legacy_handoff_without_origin_is_not_a_budget_match() {
        let temp = tempfile::TempDir::new().unwrap();
        let dir = temp.path().join("handoffs");
        fs::create_dir_all(&dir).unwrap();
        write_handoff(
            &dir.join("stage-1-handoff-001.md"),
            &HandoffV2::new("session-1", "stage-1"),
        );

        assert!(find_matching_handoff(
            "stage-1",
            "session-1",
            HandoffOrigin::BudgetExceeded,
            temp.path(),
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn unreadable_numbered_artifact_propagates_uncertainty() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("handoffs").join("stage-1-handoff-001.md");
        fs::create_dir_all(&path).unwrap();

        let error = find_matching_handoff(
            "stage-1",
            "session-1",
            HandoffOrigin::BudgetExceeded,
            temp.path(),
        )
        .unwrap_err();

        assert!(format!("{error:#}").contains("Failed to read handoff file"));
    }
}
