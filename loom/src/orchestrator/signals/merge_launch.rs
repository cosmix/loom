//! One admission lock for CLI and daemon merge resolver launches.

use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use crate::fs::locking::locked_update;
use crate::fs::session_files::{save_session, session_to_markdown};
use crate::models::session::{Session, SessionBackendKind, SessionStatus, SessionType};
use crate::parser::frontmatter::parse_from_markdown;

#[cfg(test)]
pub(crate) mod test_backend {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, OnceLock};

    type Hook = Arc<dyn Fn(Session) -> Result<Session> + Send + Sync>;
    static HOOKS: OnceLock<Mutex<HashMap<PathBuf, Hook>>> = OnceLock::new();

    pub(crate) fn install(work_dir: &Path, hook: Hook) {
        HOOKS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(work_dir.to_owned(), hook);
    }

    pub(crate) fn remove(work_dir: &Path) {
        HOOKS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .remove(work_dir);
    }

    pub(crate) fn spawn(work_dir: &Path, session: Session) -> Option<Result<Session>> {
        let hook = HOOKS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .get(work_dir)
            .cloned();
        hook.map(|hook| hook(session))
    }
}

/// Hold the stage's resolver admission lock through discovery, signal and
/// session publication, and backend launch. A process crash releases the lock;
/// the signal and write-ahead session record remain for the next discovery.
pub(crate) fn with_merge_resolver_lock<T>(
    work_dir: &Path,
    stage_id: &str,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    crate::validation::validate_id(stage_id)?;
    let directory = work_dir.join("merge-resolver-locks");
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("creating resolver lock directory {}", directory.display()))?;
    let path = directory.join(format!("{stage_id}.lock"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .with_context(|| format!("opening resolver admission lock {}", path.display()))?;
    lock_and_run(file, f)
}

fn lock_and_run<T>(file: File, f: impl FnOnce() -> Result<T>) -> Result<T> {
    file.lock_exclusive()
        .context("locking merge resolver admission")?;
    f()
}

/// Persist attribution and the actual backend before a resolver can launch.
pub(crate) fn write_ahead_merge_session(
    session: &mut Session,
    stage_id: &str,
    lane: SessionBackendKind,
    work_dir: &Path,
) -> Result<()> {
    session.assign_to_stage(stage_id.to_string());
    session.backend = lane;
    save_session(session, work_dir).context("writing merge resolver session ahead of launch")
}

/// Publish only the backend's result fields over the fresh record, preserving
/// any heartbeat or terminal transition that arrived while launch completed.
pub(crate) fn publish_merge_session(spawned: &Session, work_dir: &Path) -> Result<()> {
    let path = work_dir.join("sessions").join(format!("{}.md", spawned.id));
    locked_update(&path, |content| {
        let mut current: Session = parse_from_markdown(&content, "Session")?;
        anyhow::ensure!(
            current.id == spawned.id
                && current.stage_id == spawned.stage_id
                && current.session_type == SessionType::Merge,
            "merge resolver session identity changed during launch: {}",
            spawned.id
        );
        if current.status == SessionStatus::Spawning {
            current.pid = spawned.pid;
            current.worktree_path = spawned.worktree_path.clone();
            current.backend = spawned.backend;
            current.tracking_key = spawned.tracking_key.clone();
            current.try_mark_running()?;
        }
        Ok(session_to_markdown(&current))
    })
    .with_context(|| format!("publishing merge resolver session {}", spawned.id))
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;
    use crate::fs::session_files::load_session_exact;
    use crate::models::stage::Stage;
    use crate::orchestrator::signals::{find_live_merge_session_for_stage, generate_merge_signal};

    #[test]
    fn target_snapshot_survives_session_publication_and_target_movement() {
        use crate::git::signing::tests::git_in;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        git_in(root, &["init", "-b", "main"]);
        git_in(root, &["config", "user.name", "test"]);
        git_in(root, &["config", "user.email", "test@example.com"]);
        git_in(root, &["commit", "--allow-empty", "-m", "first"]);
        let mut session = Session::new_merge("loom/s".into(), "main".into());
        let snapshot =
            crate::orchestrator::signals::publish_merge_snapshot(root, root, &session.id, "main")
                .unwrap();
        write_ahead_merge_session(&mut session, "s", SessionBackendKind::Tmux, root).unwrap();
        git_in(root, &["commit", "--allow-empty", "-m", "later"]);
        publish_merge_session(&session, root).unwrap();
        let saved = load_session_exact(root, &session.id).unwrap().unwrap();
        assert_eq!(
            crate::orchestrator::signals::load_merge_snapshot(root, &saved.id).unwrap(),
            snapshot
        );
        assert_ne!(git_in(root, &["rev-parse", "main"]), snapshot);
    }

    #[test]
    fn discovery_waits_for_signal_and_write_ahead_publication() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stage = Stage {
            id: "mine".to_string(),
            ..Stage::default()
        };
        let mut session = Session::new_merge("loom/mine".into(), "main".into());
        let id = session.id.clone();
        let (signal_ready, ready) = mpsc::channel();
        let (release, proceed) = mpsc::channel();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                with_merge_resolver_lock(work_dir, "mine", || {
                    generate_merge_signal(&session, &stage, "loom/mine", "main", &[], work_dir)?;
                    signal_ready.send(()).unwrap();
                    proceed.recv().unwrap();
                    write_ahead_merge_session(
                        &mut session,
                        "mine",
                        SessionBackendKind::Tmux,
                        work_dir,
                    )
                })
                .unwrap();
            });
            ready.recv().unwrap();
            let (scan_started, started) = mpsc::channel();
            let (scan_done, done) = mpsc::channel();
            let scan = scope.spawn(move || {
                scan_started.send(()).unwrap();
                scan_done
                    .send(find_live_merge_session_for_stage("mine", work_dir))
                    .unwrap();
            });
            started.recv().unwrap();
            assert!(matches!(
                done.recv_timeout(std::time::Duration::from_millis(50)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ));
            release.send(()).unwrap();
            assert_eq!(done.recv().unwrap().unwrap(), Some(id.clone()));
            scan.join().unwrap();
        });
        assert!(work_dir.join("signals").join(format!("{id}.md")).exists());
    }

    #[test]
    fn failed_publication_preserves_the_launch_reservation() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stage = Stage {
            id: "mine".to_string(),
            ..Stage::default()
        };
        let mut session = Session::new_merge("loom/mine".into(), "main".into());
        generate_merge_signal(&session, &stage, "loom/mine", "main", &[], work_dir).unwrap();
        write_ahead_merge_session(&mut session, "mine", SessionBackendKind::Tmux, work_dir)
            .unwrap();
        let mut wrong = session.clone();
        wrong.stage_id = Some("other".into());
        assert!(publish_merge_session(&wrong, work_dir).is_err());
        let stored = load_session_exact(work_dir, &session.id).unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Spawning);
        assert_eq!(
            find_live_merge_session_for_stage("mine", work_dir).unwrap(),
            Some(session.id)
        );
    }

    #[test]
    fn publication_preserves_a_newer_heartbeat() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let mut session = Session::new_merge("loom/mine".into(), "main".into());
        write_ahead_merge_session(&mut session, "mine", SessionBackendKind::Tmux, work_dir)
            .unwrap();
        let mut heartbeat = session.clone();
        heartbeat.context_tokens = 42;
        save_session(&heartbeat, work_dir).unwrap();
        let mut spawned = session.clone();
        spawned.pid = Some(12345);
        publish_merge_session(&spawned, work_dir).unwrap();
        let stored = load_session_exact(work_dir, &session.id).unwrap().unwrap();
        assert_eq!(stored.status, SessionStatus::Running);
        assert_eq!(stored.pid, Some(12345));
        assert_eq!(stored.context_tokens, 42);
    }
}
