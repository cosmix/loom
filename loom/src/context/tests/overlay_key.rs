//! One overlay address, two derivations — and nothing reports them disagreeing.
//!
//! A stage's source-graph overlay is WRITTEN by
//! [`MergeLifecycle::reconcile_overlay`], which keys it by the `plan_id` in
//! `.loom/work/config.toml`, and READ by the stage's knowledge brief
//! (`orchestrator/signals/retrieval.rs`), which keys it by
//! [`plan_key`] over `Stage::plan_id`. `loom init` stamps both from one parsed
//! plan id (`commands/init/plan_setup.rs`: the `[plan]` table and every stage
//! record), which is the whole reason they agree.
//!
//! They must, because a disagreement is silent: the reader asks
//! [`crate::context::graph_store::GraphStore::resolved`] for an overlay nobody
//! wrote, and it returns the base layer with no error and no warning. The brief
//! degrades to the last merged revision, every acceptance gate still passes, and
//! the only symptom is a stage being told about code it already changed.
//!
//! So this walks the real writer and the real reader over one project, and
//! compares the addresses they actually used.

use crate::context::delivery::plan_key;
use crate::context::graph_store::{GraphStore, LAYER_FILE};
use crate::context::local_overlay::OverlayScope;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use crate::orchestrator::merge_lifecycle::MergeLifecycle;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// The single plan id `loom init` would stamp into BOTH `.loom/work/config.toml` and
/// the stage record. Supplying it once here is the invariant under test.
const PLAN_ID: &str = "PLAN-source-channel";
const STAGE_ID: &str = "source-ranker";

/// Run one git command with ambient global/system config neutralized, so a
/// developer's or CI runner's `~/.gitconfig` cannot change test behavior.
fn git_ok(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A project root with `.loom/work/config.toml` naming [`PLAN_ID`], and a committed
/// git repository at `.worktrees/<stage>` for the overlay reconcile to walk —
/// the shape [`MergeLifecycle::reconcile_overlay`] expects at merge time.
fn project_with_stage_worktree() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    fs::create_dir_all(root.join(".loom").join("work")).unwrap();
    fs::write(
        root.join(".loom").join("work").join("config.toml"),
        format!(
            "[plan]\nsource_path = \"doc/plans/PLAN-source-channel.md\"\n\
             plan_id = \"{PLAN_ID}\"\nplan_name = \"Source channel\"\n\
             base_branch = \"main\"\n"
        ),
    )
    .unwrap();

    let worktree = root.join(".worktrees").join(STAGE_ID);
    fs::create_dir_all(&worktree).unwrap();
    git_ok(&worktree, &["init", "-b", "main"]);
    git_ok(&worktree, &["config", "user.email", "t@t.com"]);
    git_ok(&worktree, &["config", "user.name", "t"]);
    fs::write(worktree.join("ranked.rs"), "pub fn rank_source() {}\n").unwrap();
    git_ok(&worktree, &["add", "ranked.rs"]);
    git_ok(&worktree, &["commit", "-m", "seed"]);

    temp
}

/// The stage record the reader holds, as `loom init` would have written it:
/// same plan id as the config, because both come from `parsed_plan.id`.
fn stage_record() -> Stage {
    let mut stage = Stage::new("Source ranker".to_string(), None);
    stage.id = STAGE_ID.to_string();
    stage.plan_id = Some(PLAN_ID.to_string());
    stage
}

/// The graph store the reader opens: cache root from [`ContextStore`], overlay
/// root from `.loom/work/`, exactly as `retrieve::load_resolved_graph` builds it.
fn reader_graph_store(project_root: &Path) -> GraphStore {
    let work_dir = WorkDir::new(project_root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    GraphStore::new(store.root(), work_dir.root())
}

/// Every overlay layer file under `.loom/work/context/`, whatever it is keyed by.
///
/// Collected by walking rather than by asking either derivation, so the writer's
/// address is observed instead of assumed — a test that asked one of the two
/// derivations where to look could not fail when they disagree.
fn written_overlay_layers(work_dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.file_name().and_then(|name| name.to_str()) == Some(LAYER_FILE) {
                found.push(path);
            }
        }
    }

    let mut found = Vec::new();
    walk(&work_dir.join("context"), &mut found);
    found.sort();
    found
}

#[test]
fn the_merge_lifecycle_writes_the_overlay_a_stage_brief_reads() {
    let temp = project_with_stage_worktree();
    let project_root = temp.path();
    let work_dir = project_root.join(".loom").join("work");

    // WRITER: the real production path, keyed off `.loom/work/config.toml`.
    MergeLifecycle::new(STAGE_ID, project_root, &work_dir).reconcile_overlay();

    // READER: the address `signals::retrieval::stage_overlay_scope` builds.
    let stage = stage_record();
    let scope = OverlayScope::Stage {
        plan: plan_key(&stage).to_string(),
        stage: stage.id.clone(),
    };
    let (plan, stage_name) = scope.resolve(project_root);
    let graph_store = reader_graph_store(project_root);

    // One layer file, at the reader's address. Asserting the whole set rather
    // than "the reader's path exists" is what also rules out the checkout-wide
    // `OverlayScope::Local` address: for signal generation that resolves to the
    // main checkout the daemon runs in, never the stage's worktree.
    assert_eq!(
        written_overlay_layers(&work_dir),
        vec![graph_store.overlay_path(&plan, &stage_name)],
        "the reconcile wrote the stage overlay somewhere the brief does not read"
    );

    // The address resolving is not enough: it must resolve to a usable layer,
    // since an empty one would read as "this stage changed nothing".
    let overlay = graph_store
        .load_overlay(&plan, &stage_name)
        .unwrap()
        .expect("the reader's address holds the layer the reconcile wrote");
    assert!(
        overlay.files.contains_key("ranked.rs"),
        "the overlay must describe the worktree's own files, got {:?}",
        overlay.files.keys().collect::<Vec<_>>()
    );
}
