//! The recorded pairs and the scratch checkout each one is measured in.
//!
//! The fixtures are byte copies of the plan's captures; `capture.py`, kept
//! beside the originals under
//! `doc/plans/token-optim/briefs/dependency-findings/dependency-findings/fixtures/`,
//! defines every field. Only the fields the harness reads are typed here.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::Deserialize;
use tempfile::TempDir;

use crate::context::freshness::GraphState;
use crate::context::graph_store::GraphStore;
use crate::context::refresh::{ensure_snapshot, SnapshotPolicy};
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus, StageType, WiringCheck};
use crate::verify::transitions::save_stage;

#[derive(Deserialize)]
pub(super) struct Pair {
    pub(super) pair: String,
    pub(super) dependency: Dependency,
    pub(super) successor: Successor,
    pub(super) changed_paths: Vec<String>,
    pub(super) transcripts: Vec<Transcript>,
}

#[derive(Deserialize)]
pub(super) struct Dependency {
    id: String,
    name: String,
    stage_type: StageType,
    working_dir: String,
    description: String,
    files: Vec<String>,
    artifacts: Vec<String>,
    completed_commit: String,
    journal: String,
}

#[derive(Deserialize)]
pub(super) struct Successor {
    id: String,
    name: String,
    stage_type: StageType,
    working_dir: String,
    description: String,
    files: Vec<String>,
    artifacts: Vec<String>,
    wiring_descriptions: Vec<String>,
    dependencies: Vec<String>,
    base_commit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Role {
    Main,
    Contract,
    Worker,
}

impl Role {
    pub(super) const ALL: [Role; 3] = [Role::Main, Role::Contract, Role::Worker];
}

#[derive(Deserialize)]
pub(super) struct Transcript {
    pub(super) id: String,
    pub(super) role: Role,
    pub(super) api_calls: u64,
    pub(super) spawn: Option<Spawn>,
    pub(super) reads: Vec<RecordedRead>,
}

#[derive(Deserialize)]
pub(super) struct Spawn {
    pub(super) subagent_type: String,
    pub(super) description: String,
    pub(super) prompt: String,
}

#[derive(Deserialize)]
pub(super) struct RecordedRead {
    pub(super) path: String,
    pub(super) first_arrival_tokens: u64,
    pub(super) replay_weighted_tokens: u64,
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dependency_findings")
}

/// Load `tests/fixtures/dependency_findings/<name>.json`.
pub(super) fn load(name: &str) -> Pair {
    let path = fixture_dir().join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

impl Dependency {
    /// The accepted dependency's record, as the orchestrator leaves it.
    pub(super) fn stage(&self) -> Stage {
        Stage {
            id: self.id.clone(),
            name: self.name.clone(),
            description: Some(self.description.clone()),
            stage_type: self.stage_type,
            working_dir: Some(self.working_dir.clone()),
            files: self.files.clone(),
            artifacts: self.artifacts.clone(),
            status: StageStatus::Completed,
            merged: true,
            merge_assumed: false,
            completed_commit: Some(self.completed_commit.clone()),
            ..Stage::default()
        }
    }
}

impl Successor {
    /// The recipient's record; wiring checks keep only their descriptions.
    pub(super) fn stage(&self) -> Stage {
        let wiring = self
            .wiring_descriptions
            .iter()
            .map(|description| WiringCheck {
                source: String::new(),
                pattern: String::new(),
                description: description.clone(),
                literal: false,
            })
            .collect();
        Stage {
            id: self.id.clone(),
            name: self.name.clone(),
            description: Some(self.description.clone()),
            stage_type: self.stage_type,
            working_dir: Some(self.working_dir.clone()),
            files: self.files.clone(),
            artifacts: self.artifacts.clone(),
            dependencies: self.dependencies.clone(),
            plan_id: Some("harness".to_string()),
            wiring,
            ..Stage::default()
        }
    }
}

/// Run git with the isolated configuration `git/merge/test_support.rs`
/// uses, so no user or system setting changes what the clone does.
fn git(dir: &Path, config_root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env(
            "GIT_CONFIG_GLOBAL",
            config_root.join(".loom-test-no-global"),
        )
        .env(
            "GIT_CONFIG_SYSTEM",
            config_root.join(".loom-test-no-system"),
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap_or_else(|error| panic!("spawn git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A shared, detached-from-origin clone at the successor's base commit, with
/// both stage records, the dependency's journal and a current base graph.
pub(super) struct Scratch {
    _temp: TempDir,
    pub(super) repo: PathBuf,
    /// The resolved `.loom/work` root.
    pub(super) work_dir: PathBuf,
}

impl Scratch {
    pub(super) fn prepare(pair: &Pair) -> Scratch {
        let temp = TempDir::new().expect("temp dir");
        let config_root = temp.path().to_path_buf();
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let toplevel = git(manifest, &config_root, &["rev-parse", "--show-toplevel"]);
        git(
            temp.path(),
            &config_root,
            &[
                "clone",
                "--quiet",
                "--shared",
                "--no-checkout",
                &toplevel,
                "repo",
            ],
        );
        let repo = temp.path().join("repo");
        let base = pair.successor.base_commit.as_str();
        git(
            &repo,
            &config_root,
            &["checkout", "--quiet", "-B", "main", base],
        );
        // The clone's origin/HEAD names the stage worktree's branch, which
        // `git::branch::default_branch` would otherwise return as the target.
        git(&repo, &config_root, &["remote", "remove", "origin"]);
        let work_dir = write_state(&repo, pair);
        Scratch {
            _temp: temp,
            repo,
            work_dir,
        }
    }

    /// Build the base source graph for HEAD and require it to be current.
    pub(super) fn build_graph(&self) {
        let started = Instant::now();
        let work_dir = WorkDir::new(&self.repo).expect("work dir");
        let store = ContextStore::open(&work_dir).expect("context store");
        let graph_store = GraphStore::new(store.root(), work_dir.root());
        let outcome = ensure_snapshot(&store, &graph_store, &self.repo, SnapshotPolicy::BaseOnly);
        assert_eq!(outcome.state(), GraphState::Current, "{outcome:?}");
        println!("graph build: {:.1} s", started.elapsed().as_secs_f64());
    }
}

/// Save both records and the dependency's journal; return the `.loom/work` root.
fn write_state(repo: &Path, pair: &Pair) -> PathBuf {
    std::fs::create_dir_all(repo.join(".loom/work/memory")).expect("state dir");
    let work_dir = WorkDir::new(repo).expect("work dir").root().to_path_buf();
    save_stage(&pair.dependency.stage(), &work_dir).expect("save dependency");
    save_stage(&pair.successor.stage(), &work_dir).expect("save successor");
    let journal = fixture_dir().join(&pair.dependency.journal);
    let target = work_dir
        .join("memory")
        .join(format!("{}.md", pair.dependency.id));
    std::fs::copy(&journal, &target).expect("copy journal");
    work_dir
}
