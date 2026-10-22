//! The two path arms, the packs production would build with each, and what
//! each pack covers of the recorded reads.

use std::path::Path;

use crate::commands::hook::worker_brief::{declared_paths, scope_pack, worker_query_text};
use crate::context::config::RetrievalConfig;
use crate::context::delivery::{dependency_chunk_ids, plan_key};
use crate::context::rank_source::normalize_dependency_path;
use crate::context::retrieve::{retrieve_for_stage, StageQuery, Surface};
use crate::context::schema::{Channel, ContextItem, ContextPack, SelectionReason};
use crate::models::stage::Stage;

use super::fixture::{RecordedRead, Role, Spawn, Transcript};

/// What `StageQuery::dependency_paths` returned before this plan
/// (`retrieval.rs:100-123` at `6fee4ecd`): every readable dependency's
/// declared `files` then `artifacts`, normalized and deduplicated, with no
/// `working_dir` join and no glob filter. Production no longer has it.
pub(super) fn old_paths(work_dir: &Path, stage: &Stage) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for id in &stage.dependencies {
        let Ok(dependency) = crate::verify::transitions::load_stage(id, work_dir) else {
            continue;
        };
        for declared in dependency.files.iter().chain(dependency.artifacts.iter()) {
            let normalized = normalize_dependency_path(declared);
            if !normalized.is_empty() && !paths.contains(&normalized) {
                paths.push(normalized);
            }
        }
    }
    paths
}

/// A query carrying everything but its text and paths, as both production
/// paths set it up (`retrieve_stage_pack`, `worker_brief::retrieve_pack`).
fn base_query(work_dir: &Path, stage: &Stage, text: String) -> StageQuery {
    let mut query = StageQuery::new(work_dir, text);
    query.overlay = StageQuery::stage_overlay_scope(stage);
    query.stage_dependency_ids =
        dependency_chunk_ids(work_dir, plan_key(stage), &stage.dependencies);
    query
}

/// Mirror of `retrieval.rs::retrieve_stage_pack` with `paths` as the arm.
pub(super) fn stage_pack(
    repo: &Path,
    work_dir: &Path,
    stage: &Stage,
    paths: &[String],
) -> ContextPack {
    let budget = RetrievalConfig::load(repo).stage_brief_budget_tokens;
    let mut query = base_query(work_dir, stage, StageQuery::build_stage_query_text(stage));
    query.dependency_paths = paths.to_vec();
    retrieve_for_stage(&query, budget).expect("stage pack retrieval")
}

/// Mirror of `worker_brief::retrieve_pack`: the arm's paths, then each path
/// the spawn prompt declares that is not already present, then the scope cut.
pub(super) fn worker_pack(
    repo: &Path,
    work_dir: &Path,
    stage: &Stage,
    spawn: &Spawn,
    paths: &[String],
) -> ContextPack {
    let budget = RetrievalConfig::load(repo).stage_brief_budget_tokens;
    let text = worker_query_text(stage, &spawn.description, &spawn.prompt);
    let mut query = base_query(work_dir, stage, text).with_surface(Surface::Hook);
    query.dependency_paths = paths.to_vec();
    for path in declared_paths(&spawn.prompt) {
        if !query.dependency_paths.contains(&path) {
            query.dependency_paths.push(path);
        }
    }
    let mut pack = retrieve_for_stage(&query, budget).expect("worker pack retrieval");
    scope_pack(&mut pack, &query.required_ids);
    pack
}

/// Whether a worker transcript gets a pack: it read something, and its spawn
/// was not Codex-bound (those get no worker brief).
pub(super) fn gets_worker_pack(transcript: &Transcript) -> bool {
    let Some(spawn) = transcript.spawn.as_ref() else {
        return false;
    };
    transcript.role == Role::Worker
        && !transcript.reads.is_empty()
        && spawn.subagent_type != "loom-codex-forwarder"
        && !spawn.subagent_type.starts_with("codex:")
}

/// Covered reads of one transcript's pack, summed.
#[derive(Debug, Default, Clone, Copy)]
pub(super) struct Coverage {
    pub(super) first_arrival: u64,
    pub(super) replay_weighted: u64,
    pub(super) files: usize,
    /// Covered files whose every matching source item carries the
    /// stage-dependency boost: reached only through the arm's paths.
    pub(super) via_dependency_only: usize,
}

impl Coverage {
    pub(super) fn add(&mut self, other: Coverage) {
        self.first_arrival += other.first_arrival;
        self.replay_weighted += other.replay_weighted;
        self.files += other.files;
        self.via_dependency_only += other.via_dependency_only;
    }
}

fn source_items_for<'a>(pack: &'a ContextPack, path: &str) -> Vec<&'a ContextItem> {
    pack.items
        .iter()
        .filter(|item| item.source == Channel::Source)
        .filter(|item| normalize_dependency_path(&item.pointer.path.to_string_lossy()) == path)
        .collect()
}

/// Credit every read whose file the pack holds any source item of. An upper
/// bound: reads are recorded per file, not per span or symbol.
pub(super) fn coverage(pack: &ContextPack, reads: &[RecordedRead]) -> Coverage {
    let mut covered = Coverage::default();
    for read in reads {
        let items = source_items_for(pack, &normalize_dependency_path(&read.path));
        if items.is_empty() {
            continue;
        }
        covered.first_arrival += read.first_arrival_tokens;
        covered.replay_weighted += read.replay_weighted_tokens;
        covered.files += 1;
        let boosted =
            |item: &&ContextItem| item.reasons.contains(&SelectionReason::StageDependency);
        if items.iter().all(boosted) {
            covered.via_dependency_only += 1;
        }
    }
    covered
}

/// Item ids the new pack gained and lost against the old one, and the lost
/// ones that were knowledge (a large change set can crowd knowledge out).
pub(super) struct PackDiff {
    pub(super) gained: Vec<String>,
    pub(super) lost: Vec<String>,
    pub(super) knowledge_lost: Vec<String>,
}

fn holds(pack: &ContextPack, item: &ContextItem) -> bool {
    pack.items.iter().any(|other| other.id == item.id)
}

pub(super) fn diff(old: &ContextPack, new: &ContextPack) -> PackDiff {
    let lost_items: Vec<&ContextItem> = old.items.iter().filter(|item| !holds(new, item)).collect();
    PackDiff {
        gained: new
            .items
            .iter()
            .filter(|item| !holds(old, item))
            .map(|item| item.id.as_str().to_string())
            .collect(),
        lost: lost_items
            .iter()
            .map(|item| item.id.as_str().to_string())
            .collect(),
        knowledge_lost: lost_items
            .iter()
            .filter(|item| item.source == Channel::Knowledge)
            .map(|item| item.id.as_str().to_string())
            .collect(),
    }
}
