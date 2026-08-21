//! End-to-end tests for [`super::reconcile_graph`] and
//! [`super::spawn_if_needed`], against real `.work/` and git fixtures.
//!
//! The debounce DECISION and lock claim mechanics they both depend on are
//! pure and tested separately, process-free, in
//! `reconcile_graph/tests_lock.rs`. Everything up to and including the
//! `claim_lock` call in `try_spawn` — the SKIP path, the inferred-root gate
//! (`allowed_to_spawn`), and the SPAWN path's lock claim — is safe to drive
//! end to end here: `spawn_detached`, the one function that would otherwise
//! launch a real, process-group-leading child, is suppressed for the whole
//! test build by `SPAWN_ENABLED` (see its doc in `reconcile_graph.rs`), so
//! reaching the SPAWN decision in a test costs nothing more than a claimed
//! lock file.
//!
//! `reconcile_graph`/`reconcile` mutate process environment
//! (`LOOM_STAGE_ID`/`LOOM_WORK_DIR`) and are therefore `#[serial]`.

use super::*;
use crate::context::schema::{Channel, Freshness, OmissionSummary};
use crate::models::stage::Stage;
use serial_test::serial;
use std::sync::atomic::Ordering;
use tempfile::TempDir;

// `read_lock` is test-only (production code never re-reads its own writes),
// so it stays out of `reconcile_graph.rs`'s top-level `use lock::{...}` —
// adding it there would be an unused import outside `#[cfg(test)]`.
use super::lock::read_lock;

// ---------------------------------------------------------------------------
// Git fixture — same pattern as `context::refresh::tests_source_graph`
// (`isolated_git`/`git_ok`/`init_repo`/`head_sha`), duplicated here because
// that module's fixture helpers are private to its own test module.
// ---------------------------------------------------------------------------

/// Run one git command with ambient global/system config neutralized, so a
/// developer's or CI runner's `~/.gitconfig` cannot change test behavior.
fn isolated_git(root: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}

fn git_ok(root: &Path, args: &[&str]) {
    let out = isolated_git(root, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A temp git repo with a `.work/` directory and one committed file, ready
/// for a checkout-scope reconcile.
fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "t@t.com"]);
    git_ok(root, &["config", "user.name", "t"]);

    std::fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
    git_ok(root, &["add", "src.rs"]);
    git_ok(root, &["commit", "-m", "seed"]);

    std::fs::create_dir_all(root.join(".work")).unwrap();
    temp
}

fn head_sha(root: &Path) -> String {
    let out = isolated_git(root, &["rev-parse", "HEAD"]);
    assert!(out.status.success(), "rev-parse HEAD failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Point this process at `root` with no stage naming it.
fn enter_checkout(root: &Path) {
    std::env::remove_var("LOOM_STAGE_ID");
    std::env::set_var("LOOM_WORK_DIR", root.join(".work"));
}

fn leave() {
    std::env::remove_var("LOOM_STAGE_ID");
    std::env::remove_var("LOOM_WORK_DIR");
}

// ---------------------------------------------------------------------------
// `reconcile_graph()` — checkout scope
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn reconcile_graph_moves_a_stale_semantic_revision_to_head() {
    let temp = init_repo();
    let root = temp.path();

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    store
        .update_state(|state| {
            state.semantic = Freshness {
                revision: "stale-revision-not-head".to_string(),
                ..Freshness::default()
            };
        })
        .unwrap();

    enter_checkout(root);
    let result = reconcile_graph();
    leave();

    assert!(result.is_ok(), "reconcile_graph must always return Ok(())");
    let state = store.load_state().unwrap();
    assert_eq!(state.semantic.revision, head_sha(root));
    assert!(
        !state.semantic.stale,
        "a freshly published base must not read as stale"
    );
}

#[test]
#[serial]
fn reconcile_graph_leaves_a_finished_marker_after_a_successful_run() {
    let temp = init_repo();
    let root = temp.path();
    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let lock_path = reconcile_lock_path(&store);
    // A pre-existing in-progress claim, as `spawn_if_needed` would have left
    // before spawning this same process.
    assert!(claim_lock(
        &lock_path,
        unix_now(),
        std::process::id(),
        false
    ));

    enter_checkout(root);
    reconcile_graph().unwrap();
    leave();

    let (_, pid) = read_lock(&lock_path)
        .expect("reconcile_graph must leave a marker behind, never unlink the lock");
    assert_eq!(pid, 0, "a completed run's marker must carry pid 0");
}

#[test]
#[serial]
fn reconcile_graph_on_a_project_with_no_git_repo_still_returns_ok_and_marks_finished() {
    // A plain directory: no `.work/`, no `.git` anywhere above it, so
    // `WorkDir::new`'s upward search finds nothing and falls back to a
    // path that does not exist on disk, and the checkout-scope reconcile's
    // own `git rev-parse HEAD` fails cleanly. Every failure on this path
    // must swallow into `Ok(())` — this is the internal-maintenance entry
    // point's whole contract — and still leave a finished marker so a
    // repeatedly-failing checkout is debounced the same as a succeeding one.
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path()).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let lock_path = reconcile_lock_path(&store);

    std::env::remove_var("LOOM_STAGE_ID");
    std::env::set_var("LOOM_WORK_DIR", temp.path());
    let result = reconcile_graph();
    leave();

    assert!(
        result.is_ok(),
        "an unresolvable target or a git failure must degrade to Ok(()), never propagate"
    );
    let (_, pid) =
        read_lock(&lock_path).expect("a failed run must still leave a finished marker behind");
    assert_eq!(pid, 0);
}

// ---------------------------------------------------------------------------
// `reconcile_graph()` — stage scope (A.22)
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn reconcile_graph_reconciles_a_named_stages_own_overlay() {
    let temp = init_repo();
    let root = temp.path();
    let work_dir_path = root.join(".work");
    let stage = Stage {
        id: "reconcile-graph-stage".to_string(),
        name: "Reconcile Graph Stage".to_string(),
        plan_id: Some("test-plan".to_string()),
        ..Stage::default()
    };
    crate::verify::transitions::create_stage(&stage, &work_dir_path).unwrap();

    std::env::remove_var("LOOM_STAGE_ID");
    std::env::set_var("LOOM_WORK_DIR", &work_dir_path);
    std::env::set_var("LOOM_STAGE_ID", &stage.id);
    let result = reconcile_graph();
    leave();

    assert!(result.is_ok());

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let overlay = graph_store
        .load_overlay("test-plan", &stage.id)
        .unwrap()
        .expect("the stage's own overlay must be written");
    assert_eq!(overlay.revision, head_sha(root));
}

// The debounce decision (`decide`) and lock claim mechanics (`claim_lock`)
// are tested in `reconcile_graph/tests_lock.rs`, wired to the `lock`
// submodule directly rather than here, so both this file and that one stay
// under the maintainability line limit.

// ---------------------------------------------------------------------------
// `spawn_if_needed` — the healthy-pack and SKIP-path cases. The SPAWN-path
// cases (the inferred-root gate, and a genuine claim reaching the suppressed
// `spawn_detached`) are covered further down, once `degraded_pack` exists.
// ---------------------------------------------------------------------------

/// A pack that would trip `spawn_if_needed`'s own `stale || degraded` gate.
fn degraded_pack() -> ContextPack {
    ContextPack {
        query: "query".to_string(),
        scope: vec![Channel::Source],
        budget_tokens: 100,
        estimated_tokens: 0,
        structural_freshness: Freshness::default(),
        semantic_freshness: Freshness::default(),
        items: Vec::new(),
        omitted: OmissionSummary::default(),
        dropped_terms: Vec::new(),
        degraded: Some("source graph base deadbeef missing".to_string()),
    }
}

#[test]
#[serial]
fn spawn_if_needed_does_nothing_when_the_pack_is_healthy() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".work")).unwrap();
    let root = temp.path();

    let healthy = ContextPack {
        degraded: None,
        semantic_freshness: Freshness::default(),
        ..degraded_pack()
    };
    spawn_if_needed(&healthy, root);

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    assert!(
        !reconcile_lock_path(&store).exists(),
        "a healthy pack must never claim the lock at all"
    );
}

#[test]
#[serial]
fn spawn_if_needed_leaves_a_young_live_lock_untouched() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".work")).unwrap();
    let root = temp.path();

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    // A legitimate spawn target (Task 2's `allowed_to_spawn` gate), same as a
    // `loom map`'d repository — without this, `try_spawn` would refuse
    // before ever reaching the lock decision this test means to exercise.
    store.ensure().unwrap();
    let lock_path = reconcile_lock_path(&store);
    // Our own pid: guaranteed alive for the duration of this test, so the
    // Skip branch is the only one `try_spawn` can take — no subprocess is
    // ever launched by this test.
    let now = unix_now();
    assert!(claim_lock(&lock_path, now, std::process::id(), false));

    spawn_if_needed(&degraded_pack(), root);

    assert_eq!(
        read_lock(&lock_path),
        Some((now, std::process::id())),
        "a young lock owned by a live pid must be left exactly as it was"
    );
}

// ---------------------------------------------------------------------------
// `allowed_to_spawn` — the inferred-root gate. A refused target must never
// even claim the debounce lock; an allowed one must reach `claim_lock` (and,
// suppressed by the test guard below, `spawn_detached`) exactly as before.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn spawn_is_refused_against_an_inferred_root_with_no_existing_cache() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".work")).unwrap();
    let root = temp.path();
    leave(); // this test's whole premise is that LOOM_WORK_DIR is unset

    spawn_if_needed(&degraded_pack(), root);

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    assert!(
        !reconcile_lock_path(&store).exists(),
        "an inferred root with no existing context cache must be refused \
         before the debounce lock is ever touched"
    );
}

#[test]
#[serial]
fn spawn_is_allowed_against_an_inferred_root_that_already_has_a_cache() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".work")).unwrap();
    let root = temp.path();
    leave();

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    // Same as a `loom map`'d repository: the context cache already exists,
    // so this root is a legitimate target even though nobody named it.
    store.ensure().unwrap();

    spawn_if_needed(&degraded_pack(), root);

    assert!(
        reconcile_lock_path(&store).exists(),
        "a root with an existing context cache must be allowed to spawn"
    );
}

#[test]
#[serial]
fn spawn_is_allowed_when_loom_work_dir_was_explicitly_set() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".work")).unwrap();
    let root = temp.path();
    std::env::set_var("LOOM_WORK_DIR", root.join(".work"));

    spawn_if_needed(&degraded_pack(), root);
    leave();

    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    assert!(
        reconcile_lock_path(&store).exists(),
        "an explicitly set LOOM_WORK_DIR must never be refused as inferred, \
         regardless of whether a context cache already exists"
    );
}

// ---------------------------------------------------------------------------
// `spawn_detached` — the process-creation guard. A test build must never
// create a real, process-group-leading child; see `SPAWN_ENABLED`'s doc.
// ---------------------------------------------------------------------------

#[test]
#[serial]
fn spawn_detached_is_suppressed_in_a_test_build() {
    let before = SUPPRESSED_SPAWNS.load(Ordering::SeqCst);

    let result = spawn_detached(Path::new("/nonexistent-reconcile-graph-test-target"));

    assert!(
        result.is_ok(),
        "a suppressed spawn must still report Ok(())"
    );
    assert_eq!(
        SUPPRESSED_SPAWNS.load(Ordering::SeqCst),
        before + 1,
        "a test build must record a suppression instead of creating a real \
         child process — a regression here is exactly the incident this \
         guard exists to prevent"
    );
}
