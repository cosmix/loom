use super::super::Config;
use crate::context::config::RetrievalConfig;
use crate::context::delivery;
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::schema::{
    Channel, ChunkId, Confidence, ContextItem, ContextPack, Coverage, FileCoverage, Freshness,
    ItemKind, LifecycleState, NodeLanguage, OmissionSummary, SelectionReason, SourceNode,
    SourceNodeKind, SourcePointer, Span,
};
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub(super) const STAGE: &str = "worker-stage";
pub(super) const PLAN: &str = "worker-plan";
pub(super) const PARENT: &str = "loom-parent";

pub(super) struct Fixture {
    pub(super) _temp: TempDir,
    pub(super) root: PathBuf,
    pub(super) config: Config,
}

impl Fixture {
    pub(super) fn new(with_overlay: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let work_dir = root.join(".loom/work");
        fs::create_dir_all(&work_dir).unwrap();
        let stage = Stage {
            id: STAGE.to_string(),
            name: "Worker admission".to_string(),
            description: Some("Retrieve narrowly relevant implementation context".to_string()),
            plan_id: Some(PLAN.to_string()),
            ..Stage::default()
        };
        if with_overlay {
            write_overlay(&root);
        }
        let retrieval = RetrievalConfig {
            stage_brief_budget_tokens: 1_000,
            max_payload_bytes: 16_384,
            ..RetrievalConfig::default()
        };
        let config = Config {
            work_dir,
            stage,
            parent_session: PARENT.to_string(),
            retrieval,
        };
        Self {
            _temp: temp,
            root,
            config,
        }
    }

    pub(super) fn transcript(&self, name: &str, body: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    pub(super) fn worker_dir(&self) -> PathBuf {
        delivery::worker_delivery_dir(&self.config.work_dir, PLAN, STAGE)
    }
}

fn write_overlay(root: &Path) {
    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let mut files = BTreeMap::new();
    for (path, symbol) in [
        ("src/scoped.rs", "ScopedWorkerMaterial"),
        ("src/unrelated.rs", "DistantQuasarWidget"),
    ] {
        let node = source_node(path, symbol);
        files.insert(
            path.to_string(),
            FileEntry {
                content_hash: format!("sha256:file-{symbol}"),
                nodes: vec![node],
                edges: Vec::new(),
                coverage: FileCoverage::Full,
            },
        );
    }
    graph_store
        .save_overlay(
            PLAN,
            STAGE,
            &GraphLayer {
                revision: "worker-overlay".to_string(),
                generation: String::new(),
                built_at: None,
                files,
                blob_index: BTreeMap::new(),
            },
        )
        .unwrap();
}

fn source_node(path: &str, symbol: &str) -> SourceNode {
    SourceNode {
        id: format!("{path}#function:{symbol}"),
        kind: SourceNodeKind::Function,
        path: PathBuf::from(path),
        scope: vec![symbol.to_string()],
        span: Span {
            start_byte: 0,
            end_byte: 40,
            line_start: 1,
            line_end: 2,
        },
        signature: format!("pub fn {symbol}()"),
        body_hash: format!("sha256:{symbol}"),
        language: NodeLanguage::Rust,
        parser_version: "test+v1".to_string(),
        coverage: FileCoverage::Full,
    }
}

pub(super) fn payload(prompt: &str) -> String {
    serde_json::json!({
        "tool_name": "Agent",
        "tool_input": {
            "subagent_type": "loom-software-engineer",
            "prompt": prompt,
            "description": "Implement scoped worker material",
            "model": "sonnet"
        },
        "session_id": "claude-parent",
        "cwd": "/project",
        "transcript_path": "/project/subagents/agent-child.jsonl"
    })
    .to_string()
}

pub(super) fn pack(revision: &str, id: &str, hash: &str, path: &str) -> ContextPack {
    ContextPack {
        query: "worker query".to_string(),
        scope: vec![Channel::Knowledge],
        budget_tokens: 500,
        estimated_tokens: 10,
        structural_freshness: Freshness {
            revision: revision.to_string(),
            ..Freshness::default()
        },
        semantic_freshness: Freshness::default(),
        items: vec![item(id, hash, path, "worker material")],
        unmet_required: Vec::new(),
        omitted: OmissionSummary {
            coverage: Coverage::default(),
            ..OmissionSummary::default()
        },
        dropped_terms: Vec::new(),
        degraded: None,
    }
}

pub(super) fn item(id: &str, hash: &str, path: &str, excerpt: &str) -> ContextItem {
    ContextItem {
        id: ChunkId::from(id),
        kind: ItemKind::KnowledgeChunk,
        pointer: SourcePointer {
            path: PathBuf::from(path),
            anchor: "worker".to_string(),
            line_start: Some(1),
            line_end: Some(2),
        },
        summary: "worker summary".to_string(),
        source: Channel::Knowledge,
        token_count: 10,
        score: 10.0,
        reasons: vec![SelectionReason::ExactPath],
        confidence: Confidence::High,
        state: LifecycleState::Active,
        content_hash: hash.to_string(),
        excerpt: Some(excerpt.to_string()),
        truncated: false,
        matched_term_count: 2,
    }
}

pub(super) fn marker(nonce: &str) -> String {
    format!("<!-- loom-worker-brief nonce={nonce} -->")
}

pub(super) fn pending_count(fixture: &Fixture) -> usize {
    fs::read_dir(fixture.worker_dir())
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    entry.path().extension().and_then(|ext| ext.to_str()) == Some("pending")
                })
                .count()
        })
        .unwrap_or(0)
}
