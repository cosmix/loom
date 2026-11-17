//! Guard coverage for [`super::RelayContext::check`].

use super::*;
use crate::relay::MAX_UNCONSUMED_TICKETS;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;

const REQUIRED_MODE: u32 = 0o700;

fn current_uid() -> u32 {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// A scratch directory named for `session_id`, owned by the current user at
/// mode 0700, inside a matching 0700 root — what `validate_session_dir`
/// requires.
fn scratch_dir(root: &TempDir, session_id: &str) -> PathBuf {
    fs::set_permissions(root.path(), fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
    let session = root.path().join(session_id);
    fs::create_dir(&session).unwrap();
    fs::set_permissions(&session, fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
    session
}

/// A `Stage` [`RelayContext`] whose scratch directory, worktree and cwd all
/// pass every guard; each test below mutates exactly one input away from it.
struct Fixture {
    _scratch_root: TempDir,
    worktree: TempDir,
    context: RelayContext,
}

fn valid_fixture() -> Fixture {
    let scratch_root = TempDir::new().unwrap();
    let scratch = scratch_dir(&scratch_root, "session-1");
    let worktree = TempDir::new().unwrap();
    let context = RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch,
        stage_id: Some("stage-a".to_string()),
        session_type: Some("stage".to_string()),
        worktree_path: Some(worktree.path().to_path_buf()),
        work_dir: None,
    };
    Fixture {
        _scratch_root: scratch_root,
        worktree,
        context,
    }
}

#[test]
fn a_fully_valid_context_is_approved() {
    let fixture = valid_fixture();
    fixture
        .context
        .check(
            RequestKind::Memory,
            None,
            fixture.worktree.path(),
            current_uid(),
        )
        .unwrap();
}

#[test]
fn refuses_a_symlinked_scratch_directory() {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
    let real = root.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(REQUIRED_MODE)).unwrap();
    let link = root.path().join("session-1");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let mut fixture = valid_fixture();
    fixture.context.scratch_dir = link;
    assert!(fixture
        .context
        .check(
            RequestKind::Memory,
            None,
            fixture.worktree.path(),
            current_uid()
        )
        .is_err());
}

#[test]
fn refuses_a_scratch_directory_whose_name_does_not_match_the_session_id() {
    let root = TempDir::new().unwrap();
    let scratch = scratch_dir(&root, "someone-else");
    let mut fixture = valid_fixture();
    fixture.context.scratch_dir = scratch;
    assert!(fixture
        .context
        .check(
            RequestKind::Memory,
            None,
            fixture.worktree.path(),
            current_uid()
        )
        .is_err());
}

#[test]
fn refuses_a_scratch_directory_with_the_wrong_mode() {
    let root = TempDir::new().unwrap();
    let scratch = scratch_dir(&root, "session-1");
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o755)).unwrap();
    let mut fixture = valid_fixture();
    fixture.context.scratch_dir = scratch;
    assert!(fixture
        .context
        .check(
            RequestKind::Memory,
            None,
            fixture.worktree.path(),
            current_uid()
        )
        .is_err());
}

#[test]
fn refuses_a_cwd_outside_the_worktree() {
    let outside = TempDir::new().unwrap();
    let fixture = valid_fixture();
    assert!(fixture
        .context
        .check(RequestKind::Memory, None, outside.path(), current_uid())
        .is_err());
}

#[test]
fn refuses_a_non_stage_cwd_outside_the_project_root() {
    let project = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let scratch_root = TempDir::new().unwrap();
    let scratch = scratch_dir(&scratch_root, "session-1");
    let context = RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch,
        stage_id: None,
        session_type: Some("knowledge".to_string()),
        worktree_path: None,
        work_dir: Some(project.path().join(".loom").join("work")),
    };
    assert!(context
        .check(RequestKind::Memory, None, outside.path(), current_uid())
        .is_err());
}

#[test]
fn refuses_a_stage_argument_that_does_not_match_the_session_stage() {
    let fixture = valid_fixture();
    assert!(fixture
        .context
        .check(
            RequestKind::Memory,
            Some("someone-elses-stage"),
            fixture.worktree.path(),
            current_uid()
        )
        .is_err());
}

#[test]
fn accepts_a_matching_stage_argument() {
    let fixture = valid_fixture();
    fixture
        .context
        .check(
            RequestKind::Memory,
            Some("stage-a"),
            fixture.worktree.path(),
            current_uid(),
        )
        .unwrap();
}

#[test]
fn refuses_an_unknown_session_type() {
    let mut fixture = valid_fixture();
    fixture.context.session_type = Some("bogus".to_string());
    assert!(fixture
        .context
        .check(
            RequestKind::Memory,
            None,
            fixture.worktree.path(),
            current_uid()
        )
        .is_err());
}

#[test]
fn refuses_a_matrix_refusal_kind_for_this_session_type() {
    let project = TempDir::new().unwrap();
    let cwd = project.path();
    let scratch_root = TempDir::new().unwrap();
    let scratch = scratch_dir(&scratch_root, "session-1");
    let context = RelayContext {
        session_id: "session-1".to_string(),
        scratch_dir: scratch,
        stage_id: None,
        session_type: Some("adjudication".to_string()),
        worktree_path: None,
        work_dir: Some(project.path().join(".loom").join("work")),
    };
    // The matrix refuses Adjudication + Memory (only a `verdict` applies).
    assert!(context
        .check(RequestKind::Memory, None, cwd, current_uid())
        .is_err());
    context
        .check(RequestKind::Verdict, None, cwd, current_uid())
        .unwrap();
}

#[test]
fn refuses_at_the_unconsumed_ticket_quota() {
    let fixture = valid_fixture();
    for n in 0..MAX_UNCONSUMED_TICKETS {
        fs::write(fixture.context.scratch_dir.join(format!("{n}.req")), b"").unwrap();
    }
    assert!(fixture
        .context
        .check(
            RequestKind::Memory,
            None,
            fixture.worktree.path(),
            current_uid()
        )
        .is_err());
}

/// Ticket-quota coverage: control tickets older than a minute stop counting
/// against the 32 limit, and nothing is ever deleted.
mod stranded_tickets {
    use super::*;
    use crate::relay::{new_request_id, Ticket, MAX_SCRATCH_TICKETS};
    use std::time::{Duration, SystemTime};

    const OLD: Duration = Duration::from_secs(120);

    fn write_ticket(scratch: &Path, kind: RequestKind, age: Duration) -> PathBuf {
        let ticket = Ticket {
            v: 1,
            id: new_request_id(),
            kind,
            created_at: Utc::now(),
            payload: serde_json::json!({}),
        };
        let path = scratch.join(format!("{}.req", ticket.id));
        fs::write(&path, ticket.encode()).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(SystemTime::now() - age)
            .unwrap();
        path
    }

    fn fill(scratch: &Path, kind: RequestKind, age: Duration, count: usize) -> Vec<PathBuf> {
        (0..count)
            .map(|_| write_ticket(scratch, kind, age))
            .collect()
    }

    fn check(fixture: &super::test_support::RelayFixture, kind: RequestKind) -> Result<()> {
        fixture
            .context
            .check(kind, None, &fixture.cwd, current_uid())
    }

    #[test]
    fn old_control_tickets_do_not_consume_the_unconsumed_quota_and_stay_on_disk() {
        let fixture = test_support::context_for(SessionType::Stage);
        let scratch = &fixture.context.scratch_dir;
        let tickets = fill(
            scratch,
            RequestKind::FileDispute,
            OLD,
            MAX_UNCONSUMED_TICKETS,
        );
        check(&fixture, RequestKind::FileDispute).unwrap();
        assert!(tickets.iter().all(|path| path.exists()));
    }

    #[test]
    fn fresh_control_tickets_count_against_the_unconsumed_quota() {
        let fixture = test_support::context_for(SessionType::Stage);
        let scratch = &fixture.context.scratch_dir;
        fill(
            scratch,
            RequestKind::FileDispute,
            Duration::ZERO,
            MAX_UNCONSUMED_TICKETS,
        );
        let error = check(&fixture, RequestKind::FileDispute)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("unconsumed relay tickets (limit 32)"),
            "{error}"
        );
        assert!(error.contains("rerun `loom check`"), "{error}");
    }

    #[test]
    fn old_swept_tickets_still_count_against_the_unconsumed_quota() {
        let fixture = test_support::context_for(SessionType::Stage);
        let scratch = &fixture.context.scratch_dir;
        fill(scratch, RequestKind::Memory, OLD, MAX_UNCONSUMED_TICKETS);
        for kind in [RequestKind::Memory, RequestKind::FileDispute] {
            let error = check(&fixture, kind).unwrap_err().to_string();
            assert!(
                error.contains("unconsumed relay tickets (limit 32)"),
                "{error}"
            );
        }
    }

    #[test]
    fn the_scratch_limit_refuses_and_names_stale_control_ids() {
        let fixture = test_support::context_for(SessionType::Stage);
        let scratch = &fixture.context.scratch_dir;
        let tickets = fill(scratch, RequestKind::Block, OLD, MAX_SCRATCH_TICKETS);
        let error = check(&fixture, RequestKind::Block).unwrap_err().to_string();
        assert!(
            error.contains(&format!("holds {MAX_SCRATCH_TICKETS} relay tickets")),
            "{error}"
        );
        let named = tickets
            .iter()
            .filter(|path| {
                let id = path.file_stem().unwrap().to_str().unwrap();
                error.contains(id)
            })
            .count();
        assert_eq!(named, 10, "{error}");
        assert!(tickets.iter().all(|path| path.exists()));
    }

    #[test]
    fn undecodable_req_files_count_as_live() {
        let fixture = test_support::context_for(SessionType::Stage);
        let scratch = &fixture.context.scratch_dir;
        for n in 0..MAX_UNCONSUMED_TICKETS {
            fs::write(scratch.join(format!("{n}.req")), b"not a ticket").unwrap();
        }
        assert!(check(&fixture, RequestKind::Block).is_err());
    }

    #[test]
    fn a_symlinked_req_counts_as_live_and_is_not_followed() {
        let fixture = test_support::context_for(SessionType::Stage);
        let scratch = &fixture.context.scratch_dir;
        let target = write_ticket(scratch, RequestKind::Block, OLD);
        let hidden = scratch.join("target.tmp");
        fs::rename(&target, &hidden).unwrap();
        for n in 0..MAX_UNCONSUMED_TICKETS {
            std::os::unix::fs::symlink(&hidden, scratch.join(format!("{n}.req"))).unwrap();
        }
        // Followed, each link would decode as an old control ticket and free the quota.
        assert!(check(&fixture, RequestKind::Block).is_err());
    }
}
