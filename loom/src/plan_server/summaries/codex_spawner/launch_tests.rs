//! What a job starts with: an id the summary commands accept, its grants, its log path, its
//! prompt and the configuration it must leave alone.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::os::unix::fs::PermissionsExt;

use serial_test::serial;

use super::*;
use crate::plan_server::context::BindInfo;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::tests::support::project;
use crate::planning::runtime::{project_key, runtime_paths_from};
use crate::planning::watch::OwnWrites;

#[test]
fn an_issued_session_id_passes_the_state_check_of_the_summary_commands() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let base = temp.path().canonicalize().expect("canonical temp dir");
    let (root, planning_root) = (base.join("project"), base.join("plan"));
    std::fs::create_dir_all(&root).expect("project");
    let state = planning_root.join(project_key(&root).expect("key"));
    std::fs::create_dir_all(&state).expect("state");
    let spawner = CodexSummarySpawner {
        ctx: Weak::new(),
        counter: AtomicU64::new(0),
        jobs: Jobs::default(),
    };
    let session = spawner.next_session().expect("session id");
    assert!(is_codex_session(&session), "{session}");

    let mode = |bits| std::fs::Permissions::from_mode(bits);
    std::fs::set_permissions(&planning_root, mode(0o555)).expect("read-only parent");
    let (state_dir, id) = (Some(state.as_path()), Some(session.as_str()));
    let resolved = runtime_paths_from(state_dir, id, &base.join("home"), &root);
    std::fs::set_permissions(&planning_root, mode(0o755)).expect("writable parent");
    assert_eq!(resolved.expect("the id is accepted").dir, state);
}

/// A context whose runtime directory sits under `home`, which it only reads at construction.
fn context_under(home: &Path, root: &Path) -> Arc<PlanContext> {
    let old = std::env::var_os("LOOM_HOME");
    std::env::set_var("LOOM_HOME", home);
    let bind = BindInfo::new(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0), "test");
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(root.to_path_buf(), read_only, bind, OwnWrites::default());
    match old {
        Some(value) => std::env::set_var("LOOM_HOME", value),
        None => std::env::remove_var("LOOM_HOME"),
    }
    Arc::new(ctx.expect("context"))
}

#[test]
#[serial]
fn prepare_creates_the_grants_and_grants_nothing_else_of_the_runtime() {
    let (home, root) = (tempfile::TempDir::new().expect("home"), project());
    let ctx = context_under(home.path(), root.path());
    let runtime = ctx.runtime().clone();
    let spawner = CodexSummarySpawner::new(&ctx);

    let launch = spawner
        .prepare(&ctx, &["plans/a".to_owned()])
        .expect("prepare");

    assert!(runtime.drafts.is_dir(), "--add-dir needs drafts/");
    assert!(runtime.claims.is_dir(), "--add-dir needs claims/");
    assert!(runtime.summaries_lock.is_file(), "the job cannot create it");
    let granted: Vec<&str> = launch
        .args
        .windows(2)
        .filter(|pair| pair[0] == "--add-dir")
        .map(|pair| pair[1].as_str())
        .collect();
    let (drafts, claims) = (runtime.drafts.display(), runtime.claims.display());
    assert_eq!(granted, [drafts.to_string(), claims.to_string()]);
    assert_eq!(spawner.log_path(&launch.session), Some(launch.log));
}

#[test]
#[serial]
fn only_a_codex_session_has_a_log_path() {
    let (home, root) = (tempfile::TempDir::new().expect("home"), project());
    let ctx = context_under(home.path(), root.path());
    let spawner = CodexSummarySpawner::new(&ctx);
    assert_eq!(
        spawner.log_path("0f8fad5b-d9cb-469f-a165-70867728950e"),
        None
    );
    assert_eq!(
        spawner.log_path("c0de-1-0"),
        Some(ctx.runtime().dir.join("codex-logs/c0de-1-0.log"))
    );
}

#[test]
fn a_changed_agents_file_fails_the_job() {
    for file in ["AGENTS.md", "AGENTS.override.md"] {
        let root = tempfile::TempDir::new().expect("temp dir");
        let before = config_guard::snapshot(root.path()).expect("config snapshot");
        let codex_before = codex_snapshot(root.path()).expect("codex snapshot");
        assert_eq!(guarded_outcome(root.path(), &before, &codex_before), None);
        std::fs::write(root.path().join(file), "new instructions\n").expect("write");
        let changed = "codex changed the .claude, .codex or AGENTS.md configuration";
        assert_eq!(
            guarded_outcome(root.path(), &before, &codex_before),
            Some(EndOutcome::Failed(changed.to_owned())),
            "{file}"
        );
    }
}

#[test]
fn the_prompt_asks_only_for_what_render_prompt_supplies() {
    let prompt = render_prompt(&["plans/a".to_owned()], Path::new("/runtime"));
    for absent in ["supplied in the assignment", "correction", "--note"] {
        assert!(!prompt.contains(absent), "{absent}: {prompt}");
    }
}
