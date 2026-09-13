//! `loom stage adjudicate` — hand an adjudication session's verdict to the
//! orchestrator.
//!
//! The adjudication session writes its JSON verdict to a draft file and hands
//! that file to this command, which validates it. What happens next depends on
//! how the session was started ([`crate::relay::emit::mode`]):
//!
//! * Relay — a session with a scratch directory cannot write the state
//!   directory. The draft must be `$LOOM_SCRATCH_DIR/verdict-<n>.json`; the
//!   command relays it as a `verdict` ticket, and the daemon records it through
//!   [`crate::orchestrator::adjudication::record`] once it has confirmed the
//!   ticket came from the live adjudication session for the stage.
//! * Legacy and operator — the command records `verdict.md` itself through
//!   that same guarded function.
//!
//! The four guards that keep a verdict narrow live with the recording logic
//! (`orchestrator/adjudication/record.rs`), so both routes enforce them.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use crate::orchestrator::adjudication::{record, scratch_verdict_draft, verdict};
use crate::relay::emit::{mode, EnvSnapshot, RelayContext, RelayMode, RelaySink, StdSink};
use crate::relay::{RequestKind, VerdictRequest};

pub use crate::orchestrator::adjudication::record::{record_verdict, AdjudicateOutcome};

/// `loom stage adjudicate --stage <id> --dispute <n> --verdict-file <path>`.
pub fn adjudicate(stage_id: String, dispute_id: u32, verdict_path: PathBuf) -> Result<()> {
    let env = EnvSnapshot::from_process_env();
    adjudicate_in(mode(&env), &env, &stage_id, dispute_id, &verdict_path)
}

/// The command body, with the relay mode decided by the caller.
fn adjudicate_in(
    relay_mode: RelayMode,
    env: &EnvSnapshot,
    stage_id: &str,
    dispute_id: u32,
    verdict_path: &Path,
) -> Result<()> {
    match relay_mode {
        RelayMode::Relay(context) => {
            let cwd = std::env::current_dir().context("Failed to resolve the current directory")?;
            // SAFETY: `getuid` has no preconditions and cannot fail.
            let uid = unsafe { libc::getuid() };
            let mut sink = StdSink::default();
            relay_verdict(
                &context,
                stage_id,
                dispute_id,
                verdict_path,
                &cwd,
                uid,
                &mut sink,
            )
        }
        RelayMode::Legacy | RelayMode::Operator => {
            record_here(env, stage_id, dispute_id, verdict_path)
        }
    }
}

/// Legacy and operator mode: record `verdict.md` directly.
fn record_here(
    env: &EnvSnapshot,
    stage_id: &str,
    dispute_id: u32,
    verdict_path: &Path,
) -> Result<()> {
    let worktree = env
        .worktree_path
        .as_deref()
        .map(|path| path.to_string_lossy().into_owned());
    record::refuse_worktree_session(worktree.as_deref())?;

    let work_dir = crate::commands::common::work_dir_path()?;
    let session_id = env.session_id.clone().filter(|s| !s.is_empty());
    match record_verdict(&work_dir, stage_id, dispute_id, verdict_path, session_id)? {
        AdjudicateOutcome::Recorded => {
            println!(
                "Recorded the verdict for stage '{stage_id}' dispute {dispute_id}. The \
                 orchestrator applies it on its next poll and then closes this session; \
                 nothing further is needed here."
            );
        }
        AdjudicateOutcome::Escalated(reason) => {
            println!("Stage '{stage_id}' was escalated to NeedsHumanReview: {reason}");
        }
    }
    Ok(())
}

/// Relay mode: validate the judge's scratch draft here, then hand it to the
/// daemon as a `verdict` ticket. Nothing under the state directory is opened.
fn relay_verdict(
    context: &RelayContext,
    stage_id: &str,
    dispute_id: u32,
    verdict_path: &Path,
    cwd: &Path,
    uid: u32,
    sink: &mut dyn RelaySink,
) -> Result<()> {
    let draft = scratch_verdict_draft(&context.scratch_dir, dispute_id);
    require_scratch_draft(verdict_path, &draft)?;
    let raw = std::fs::read_to_string(&draft)
        .with_context(|| format!("Failed to read verdict file: {}", draft.display()))?;
    if let verdict::ValidationOutcome::Escalate { reason } = verdict::parse_and_validate(&raw) {
        writeln!(
            sink.stderr(),
            "This verdict cannot be acted on as written ({reason}). Once relayed, the \
             orchestrator escalates stage '{stage_id}' to NeedsHumanReview instead of \
             recording it."
        )
        .context("failed to write the verdict notice")?;
    }

    context.check(RequestKind::Verdict, Some(stage_id), cwd, uid)?;
    let payload = serde_json::to_value(VerdictRequest {
        dispute_id,
        verdict: raw,
    })
    .context("failed to encode the verdict request")?;
    context.emit(RequestKind::Verdict, payload, "verdict", true, sink)?;
    Ok(())
}

/// In relay mode the draft lives in the session's own scratch directory, the
/// one place its sandbox lets it write. A `--verdict-file` naming anywhere
/// else is refused rather than read, so the verdict relayed is always the one
/// the instructions told the judge to write.
fn require_scratch_draft(given: &Path, draft: &Path) -> Result<()> {
    let same = given == draft
        || matches!(
            (given.canonicalize(), draft.canonicalize()),
            (Ok(given), Ok(draft)) if given == draft
        );
    if !same {
        bail!(
            "This session's verdict draft is {draft}, but --verdict-file names {given}. Write \
             the JSON verdict to {draft} and run the command again with that path.",
            draft = draft.display(),
            given = given.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::Ticket;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[derive(Default)]
    struct BufferSink {
        out: Vec<u8>,
        err: Vec<u8>,
    }

    impl RelaySink for BufferSink {
        fn stdout(&mut self) -> &mut dyn Write {
            &mut self.out
        }
        fn stderr(&mut self) -> &mut dyn Write {
            &mut self.err
        }
    }

    struct Judge {
        _tmp: tempfile::TempDir,
        project: PathBuf,
        context: RelayContext,
    }

    fn judge(session_type: &str) -> Judge {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        std::fs::create_dir_all(project.join(".loom").join("work")).unwrap();
        let scratch = tmp.path().join("scratch").join("session-judge");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::set_permissions(&scratch, std::fs::Permissions::from_mode(0o700)).unwrap();
        let context = RelayContext {
            session_id: "session-judge".to_string(),
            scratch_dir: scratch,
            stage_id: Some("s1".to_string()),
            session_type: Some(session_type.to_string()),
            worktree_path: None,
            work_dir: Some(project.join(".loom").join("work")),
        };
        Judge {
            _tmp: tmp,
            project,
            context,
        }
    }

    fn uid() -> u32 {
        // SAFETY: `getuid` has no preconditions and cannot fail.
        unsafe { libc::getuid() }
    }

    fn write_draft(judge: &Judge, body: &str) -> PathBuf {
        let draft = scratch_verdict_draft(&judge.context.scratch_dir, 1);
        std::fs::write(&draft, body).unwrap();
        draft
    }

    fn tickets(judge: &Judge) -> Vec<Ticket> {
        std::fs::read_dir(&judge.context.scratch_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "req"))
            .map(|path| Ticket::decode(&std::fs::read(path).unwrap()).unwrap())
            .collect()
    }

    const REJECT: &str = r#"{"verdict":"reject","reasoning":"right","citations":[{"file":"a","excerpt":"b","claim":"c"}]}"#;

    #[test]
    fn relay_mode_relays_the_scratch_draft_as_a_verdict_ticket() {
        let judge = judge("adjudication");
        let draft = write_draft(&judge, REJECT);
        let mut sink = BufferSink::default();

        relay_verdict(
            &judge.context,
            "s1",
            1,
            &draft,
            &judge.project,
            uid(),
            &mut sink,
        )
        .unwrap();

        let tickets = tickets(&judge);
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].kind, RequestKind::Verdict);
        let request: VerdictRequest = serde_json::from_value(tickets[0].payload.clone()).unwrap();
        assert_eq!(
            request,
            VerdictRequest {
                dispute_id: 1,
                verdict: REJECT.to_string()
            }
        );
        let stdout = String::from_utf8(sink.out).unwrap();
        assert!(stdout
            .lines()
            .last()
            .unwrap()
            .starts_with("LOOM_RELAY_V1 kind=verdict "));
        assert!(!judge.project.join(".loom/work/disputes").exists());
    }

    #[test]
    fn relay_mode_refuses_a_verdict_file_outside_the_scratch_directory() {
        let judge = judge("adjudication");
        write_draft(&judge, REJECT);
        let elsewhere = judge.project.join("verdict.json");
        std::fs::write(&elsewhere, REJECT).unwrap();

        let err = relay_verdict(
            &judge.context,
            "s1",
            1,
            &elsewhere,
            &judge.project,
            uid(),
            &mut BufferSink::default(),
        )
        .unwrap_err();

        assert!(format!("{err:#}").contains("verdict draft is"));
        assert!(tickets(&judge).is_empty());
    }

    #[test]
    fn relay_mode_refuses_a_session_kind_that_may_not_relay_a_verdict() {
        let judge = judge("knowledge");
        let draft = write_draft(&judge, REJECT);

        let err = relay_verdict(
            &judge.context,
            "s1",
            1,
            &draft,
            &judge.project,
            uid(),
            &mut BufferSink::default(),
        )
        .unwrap_err();

        assert!(format!("{err:#}").contains("may not relay"));
        assert!(tickets(&judge).is_empty());
    }

    #[test]
    fn a_degenerate_verdict_is_relayed_with_an_escalation_notice() {
        let judge = judge("adjudication");
        let draft = write_draft(
            &judge,
            r#"{"verdict":"needs-more-evidence","questions":[]}"#,
        );
        let mut sink = BufferSink::default();

        relay_verdict(
            &judge.context,
            "s1",
            1,
            &draft,
            &judge.project,
            uid(),
            &mut sink,
        )
        .unwrap();

        assert_eq!(tickets(&judge).len(), 1);
        assert!(String::from_utf8(sink.err)
            .unwrap()
            .contains("NeedsHumanReview"));
    }
}
