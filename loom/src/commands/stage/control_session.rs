//! Routing for sandbox verification and trusted broker completion requests.

use super::super::completion_evidence::{
    broker::{run_broker, BrokerOutcome, ProductionTransport},
    MAX_BROKER_INPUT_BYTES, TOOL_STATUS_ENV,
};
use super::control_complete;
use crate::daemon::DaemonServer;
use crate::fs::session_files::load_session_exact;
use crate::fs::work_dir::WorkDir;
use crate::models::session::SessionType;
use crate::models::stage::{Stage, StageType};
use crate::verify::transitions::load_stage;
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Serve a `LOOM_CONTROL_BROKER=1` invocation, returning whether it was
/// handled (in which case the caller must not continue into the ordinary
/// completion pipeline).
pub(super) fn handle_broker_request(
    stage_id: &str,
    session_id: Option<&str>,
    has_privileged_flags: bool,
    work_dir: &Path,
) -> Result<bool> {
    if !control_complete::broker_requested() {
        return Ok(false);
    }
    if has_privileged_flags {
        bail!("trusted completion broker does not accept privileged flags");
    }
    let session_id = session_id.context("trusted completion broker requires --session")?;
    require_wrapper_identity(stage_id, session_id)?;
    let outcome = load_and_run_broker(stage_id, session_id, work_dir)
        .unwrap_or_else(|error| BrokerOutcome::Uncertain(error.to_string()));
    println!("{}", outcome.outcome_line());
    if matches!(
        outcome,
        BrokerOutcome::Accepted | BrokerOutcome::AcceptedReconciled
    ) {
        Ok(true)
    } else {
        bail!("trusted completion broker outcome: {}", outcome.token())
    }
}

fn load_and_run_broker(stage_id: &str, session_id: &str, work_dir: &Path) -> Result<BrokerOutcome> {
    let (output, oversized) = read_broker_input()?;
    let stage = load_stage(stage_id, work_dir)?;
    let session = load_session_exact(work_dir, session_id)?
        .context("trusted completion broker session record is missing")?;
    if stage.session.as_deref() != Some(session_id) || session.stage_id.as_deref() != Some(stage_id)
    {
        bail!("trusted completion broker stage/session assignment mismatch");
    }
    let repo_root = WorkDir::new(work_dir)?
        .main_project_root()
        .context("failed to resolve the main project root")?;
    let failed = !oversized && !matches!(std::env::var(TOOL_STATUS_ENV).as_deref(), Ok("ok"));
    let transport = ProductionTransport::new(
        stage_id,
        session_id,
        work_dir,
        control_complete::request_completion,
    );
    Ok(run_broker(
        &stage, &session, work_dir, &repo_root, failed, &output, &transport,
    ))
}

fn read_broker_input() -> Result<(String, bool)> {
    read_broker_input_from(std::io::stdin())
}

fn read_broker_input_from(input: impl Read) -> Result<(String, bool)> {
    let limit = u64::try_from(MAX_BROKER_INPUT_BYTES + 1)
        .context("broker input limit does not fit in u64")?;
    let mut bytes = Vec::new();
    input
        .take(limit)
        .read_to_end(&mut bytes)
        .context("failed to read trusted completion broker input")?;
    let oversized = bytes.len() > MAX_BROKER_INPUT_BYTES;
    if oversized {
        bytes.clear();
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), oversized))
}

fn require_wrapper_identity(stage_id: &str, session_id: &str) -> Result<()> {
    let env_stage = std::env::var("LOOM_STAGE_ID").context("LOOM_STAGE_ID is missing")?;
    let env_session = std::env::var("LOOM_SESSION_ID").context("LOOM_SESSION_ID is missing")?;
    if env_stage != stage_id || env_session != session_id {
        bail!("trusted completion broker identity does not match wrapper identity");
    }
    Ok(())
}

/// Whether `path` names a loom worktree root — `<repo>/.worktrees/<stage-id>`.
///
/// Purely structural (no filesystem access) because it decides *routing*, not
/// authorization: [`sandbox_control_session`] still canonicalizes the path and
/// requires the working directory to sit inside it. The path must END at the
/// stage id, which is exactly what the wrapper exports: the bare container
/// directory is out, and so is a repo that itself lives under an outer
/// `.worktrees/<id>/`. Same anchored rule as `loom-hooks/loom-control-complete.sh`.
pub(super) fn is_loom_worktree_path(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|dir| dir == ".worktrees")
}

/// The wrapper variables completion routing reads, captured once so the
/// routing itself is testable without touching the process environment.
#[derive(Debug, Default)]
struct WrapperEnv {
    stage_id: Option<String>,
    session_id: Option<String>,
    worktree_path: Option<String>,
    session_type: Option<String>,
}

impl WrapperEnv {
    fn from_process_env() -> Self {
        Self {
            stage_id: std::env::var("LOOM_STAGE_ID").ok(),
            session_id: std::env::var("LOOM_SESSION_ID").ok(),
            worktree_path: std::env::var("LOOM_WORKTREE_PATH").ok(),
            session_type: std::env::var("LOOM_SESSION_TYPE").ok(),
        }
    }

    fn is_knowledge_session(&self) -> bool {
        self.session_type.as_deref() == Some(SessionType::Knowledge.to_string().as_str())
    }
}

/// The session id this completion is acting for, when it is a sandboxed
/// session whose only authority is verification. `None` means an ordinary
/// host-side completion.
pub(super) fn sandbox_control_session(
    stage: &Stage,
    stage_id: &str,
    requested_session: Option<&str>,
    work_dir: &Path,
) -> Result<Option<String>> {
    let env = WrapperEnv::from_process_env();
    if env.stage_id.is_none() || env.session_id.is_none() {
        return Ok(None);
    }
    let cwd = std::env::current_dir().context("failed to resolve completion working directory")?;
    let session = route_control_session(&env, stage, stage_id, requested_session, work_dir, &cwd)?;
    if session.is_some() && !DaemonServer::is_running(work_dir) {
        bail!("sandboxed completion requires the loom daemon to be running");
    }
    Ok(session)
}

/// [`sandbox_control_session`]'s decision, from an explicit environment and
/// working directory.
fn route_control_session(
    env: &WrapperEnv,
    stage: &Stage,
    stage_id: &str,
    requested_session: Option<&str>,
    work_dir: &Path,
    cwd: &Path,
) -> Result<Option<String>> {
    let (Some(env_stage), Some(env_session)) = (env.stage_id.as_deref(), env.session_id.as_deref())
    else {
        return Ok(None);
    };
    let claim = Claim {
        env_stage,
        env_session,
        stage_id,
        requested_session,
    };
    if env.is_knowledge_session() {
        claim.require(stage)?;
        require_knowledge_checkout(stage, work_dir, cwd)?;
        return Ok(Some(env_session.to_string()));
    }
    let Some(worktree) = env.worktree_path.as_deref() else {
        return Ok(None);
    };
    // Membership, not presence. A loom worktree lives at
    // `<repo>/.worktrees/<stage-id>/`; the main repo root does not. Sessions
    // that run in the main repo without `LOOM_SESSION_TYPE=knowledge` (merge,
    // base-conflict, and knowledge sessions spawned before that variable was
    // read here) complete through the ordinary in-process path, so a bare "the
    // variable is set" test would route them into a sandboxed wrapper route
    // that cannot serve them. Mirrors `loom_current_worktree()` in
    // `loom-hooks/_common.sh`, which has always required this.
    if !is_loom_worktree_path(Path::new(worktree)) {
        return Ok(None);
    }
    claim.require(stage)?;
    let worktree = PathBuf::from(worktree)
        .canonicalize()
        .context("failed to resolve LOOM_WORKTREE_PATH")?;
    let cwd = cwd
        .canonicalize()
        .context("failed to resolve current directory")?;
    if !cwd.starts_with(&worktree) {
        bail!("wrapper completion must run inside its assigned worktree");
    }
    Ok(Some(env_session.to_string()))
}

/// Who the wrapper says is completing, and what the command line asked for.
struct Claim<'a> {
    env_stage: &'a str,
    env_session: &'a str,
    stage_id: &'a str,
    requested_session: Option<&'a str>,
}

impl Claim<'_> {
    /// The completion names the wrapper's own stage, that stage is still
    /// assigned to the wrapper's session, and any `--session` agrees.
    fn require(&self, stage: &Stage) -> Result<()> {
        if self.env_stage != self.stage_id || stage.session.as_deref() != Some(self.env_session) {
            bail!("completion request does not match the active wrapper stage/session");
        }
        if self
            .requested_session
            .is_some_and(|requested| requested != self.env_session)
        {
            bail!("--session does not match the active wrapper session");
        }
        Ok(())
    }
}

/// A knowledge session completes only a knowledge stage, and only from inside
/// the main project root, where its acceptance runs.
fn require_knowledge_checkout(stage: &Stage, work_dir: &Path, cwd: &Path) -> Result<()> {
    if stage.stage_type != StageType::Knowledge {
        bail!("a knowledge session may complete only a knowledge stage");
    }
    let root = WorkDir::new(work_dir)?
        .main_project_root()
        .context("failed to resolve the main project root")?
        .canonicalize()
        .context("failed to resolve the main project root")?;
    let cwd = cwd
        .canonicalize()
        .context("failed to resolve current directory")?;
    if !cwd.starts_with(&root) {
        bail!("knowledge completion must run inside the main project root");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_loom_worktree_path, read_broker_input_from, route_control_session, WrapperEnv};
    use crate::commands::stage::completion_evidence::MAX_BROKER_INPUT_BYTES;
    use crate::models::stage::{Stage, StageType};
    use std::path::{Path, PathBuf};

    #[test]
    fn worktree_membership_is_structural_not_presence() {
        assert!(is_loom_worktree_path(Path::new(
            "/home/dev/repo/.worktrees/build-api"
        )));
        assert!(is_loom_worktree_path(Path::new(
            "/home/dev/outer/.worktrees/outer-stage/repo/.worktrees/build-api"
        )));

        assert!(!is_loom_worktree_path(Path::new("/home/dev/repo")));
        assert!(!is_loom_worktree_path(Path::new("/")));
        assert!(!is_loom_worktree_path(Path::new(
            "/home/dev/outer/.worktrees/outer-stage/repo"
        )));

        assert!(!is_loom_worktree_path(Path::new(
            "/home/dev/repo/.worktrees/build-api/src/nested"
        )));

        assert!(!is_loom_worktree_path(Path::new(
            "/home/dev/repo/.worktrees"
        )));

        assert!(!is_loom_worktree_path(Path::new(
            "/home/dev/repo/my.worktrees-backup"
        )));
    }

    fn knowledge_env(session: &str) -> WrapperEnv {
        WrapperEnv {
            stage_id: Some("notes".to_string()),
            session_id: Some(session.to_string()),
            worktree_path: None,
            session_type: Some("knowledge".to_string()),
        }
    }

    fn knowledge_stage(owner: &str) -> Stage {
        Stage {
            id: "notes".to_string(),
            stage_type: StageType::Knowledge,
            session: Some(owner.to_string()),
            ..Stage::default()
        }
    }

    fn project() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = tmp.path().join(".loom").join("work");
        std::fs::create_dir_all(&work_dir).unwrap();
        (tmp, work_dir)
    }

    #[test]
    fn oversized_broker_input_is_marked_missing() {
        let input = vec![b'x'; MAX_BROKER_INPUT_BYTES + 1];

        let (output, oversized) = read_broker_input_from(input.as_slice()).unwrap();

        assert_eq!((output, oversized), (String::new(), true));
    }

    #[test]
    fn a_knowledge_session_completes_through_the_broker_from_the_project_root() {
        let (tmp, work_dir) = project();
        let env = knowledge_env("session-k");

        let routed = route_control_session(
            &env,
            &knowledge_stage("session-k"),
            "notes",
            None,
            &work_dir,
            tmp.path(),
        )
        .unwrap();

        assert_eq!(routed.as_deref(), Some("session-k"));
    }

    #[test]
    fn a_knowledge_session_is_refused_outside_its_root_its_stage_or_its_kind() {
        let (tmp, work_dir) = project();
        let elsewhere = tempfile::tempdir().unwrap();
        let env = knowledge_env("session-k");
        let mine = knowledge_stage("session-k");
        let not_knowledge = Stage {
            id: "notes".to_string(),
            session: Some("session-k".to_string()),
            ..Stage::default()
        };

        assert!(
            route_control_session(&env, &mine, "notes", None, &work_dir, elsewhere.path()).is_err()
        );
        let theirs = knowledge_stage("session-other");
        assert!(
            route_control_session(&env, &theirs, "notes", None, &work_dir, tmp.path()).is_err()
        );
        assert!(
            route_control_session(&env, &not_knowledge, "notes", None, &work_dir, tmp.path())
                .is_err()
        );
        let other_session = Some("session-other");
        assert!(
            route_control_session(&env, &mine, "notes", other_session, &work_dir, tmp.path())
                .is_err()
        );
    }

    #[test]
    fn a_session_without_a_session_type_keeps_the_in_process_path() {
        let (tmp, work_dir) = project();
        let env = WrapperEnv {
            session_type: None,
            ..knowledge_env("session-k")
        };

        let routed = route_control_session(
            &env,
            &knowledge_stage("session-k"),
            "notes",
            None,
            &work_dir,
            tmp.path(),
        )
        .unwrap();

        assert_eq!(routed, None);
    }
}
