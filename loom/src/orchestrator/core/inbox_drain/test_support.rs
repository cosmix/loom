//! Fixtures for the inbox drain tests: a state directory with session and
//! stage records, entries relayed through the real inbox writer, and a host
//! whose liveness and merge finalization are fixed. Nothing here reads or
//! writes the process environment.

use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tempfile::TempDir;

use crate::fs::inbox::{read_ledger, write_entry, LedgerOutcome, LedgerRecord, WriteOutcome};
use crate::fs::memory::{MemoryEntry, MemoryEntryType};
use crate::fs::session_files::save_session;
use crate::models::dispute::{request_file, DisputeRequest};
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::schema::AcceptanceCriterion;
use crate::relay::{new_request_id, AgentRole, InboxEntry, RequestKind};
use crate::verify::transitions::save_stage;

use super::{InboxHost, Settle, Tick};

pub(super) const STAGE: &str = "s1";

/// A verdict that records cleanly.
pub(super) const REJECT: &str = r#"{"verdict":"reject","reasoning":"the criterion is right","citations":[{"file":"src/a.rs","excerpt":"fn a","claim":"exists"}]}"#;

pub(super) struct FakeHost {
    pub work_dir: PathBuf,
    pub repo_root: PathBuf,
    pub alive: bool,
    pub merges: Vec<(String, String)>,
    reported: HashSet<String>,
}

impl InboxHost for FakeHost {
    fn work_dir(&self) -> &Path {
        &self.work_dir
    }
    fn repo_root(&self) -> &Path {
        &self.repo_root
    }
    fn session_alive(&self, _session: &Session) -> Result<bool> {
        Ok(self.alive)
    }
    fn resolve_merge(&mut self, session: &Session, stage_id: &str) -> Settle {
        self.merges.push((session.id.clone(), stage_id.to_string()));
        Settle::Applied(None)
    }
    fn first_report(&mut self, key: &str) -> bool {
        self.reported.insert(key.to_string())
    }
    fn clear_report(&mut self, key: &str) {
        self.reported.remove(key);
    }
}

pub(super) struct Fixture {
    _tmp: TempDir,
    pub repo_root: PathBuf,
    pub work_dir: PathBuf,
    pub scratch_root: PathBuf,
}

pub(super) fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let work_dir = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    std::fs::create_dir_all(work_dir.join("sessions")).unwrap();
    let scratch_root = tmp.path().join("scratch");
    std::fs::create_dir_all(&scratch_root).unwrap();
    std::fs::set_permissions(&scratch_root, std::fs::Permissions::from_mode(0o700)).unwrap();
    Fixture {
        _tmp: tmp,
        repo_root,
        work_dir,
        scratch_root,
    }
}

impl Fixture {
    pub fn host(&self, alive: bool) -> FakeHost {
        FakeHost {
            work_dir: self.work_dir.clone(),
            repo_root: self.repo_root.clone(),
            alive,
            merges: Vec::new(),
            reported: HashSet::new(),
        }
    }

    pub fn tick(&self, now: DateTime<Utc>) -> Tick<'_> {
        Tick {
            scratch_root: Some(&self.scratch_root),
            now,
        }
    }

    /// A session record working [`STAGE`].
    pub fn record(&self, session_type: SessionType, status: SessionStatus) -> Session {
        let mut session = Session::new();
        session.session_type = session_type;
        session.assign_to_stage(STAGE.to_string());
        session.status = status;
        save_session(&session, &self.work_dir).unwrap();
        session
    }

    /// [`STAGE`] in `status`, owned by `owner`, with one acceptance criterion.
    pub fn stage(&self, status: StageStatus, owner: Option<&str>) {
        let mut stage = Stage::new(STAGE.to_string(), None);
        stage.id = STAGE.to_string();
        stage.status = status;
        stage.session = owner.map(str::to_string);
        stage.acceptance = vec![AcceptanceCriterion::Simple("cargo test".to_string())];
        save_stage(&stage, &self.work_dir).unwrap();
    }

    pub fn file_dispute(&self, dispute_id: u32) {
        let disputes = self.work_dir.join("disputes");
        let path = request_file(&disputes, STAGE, dispute_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let request = DisputeRequest {
            id: dispute_id,
            stage_id: STAGE.to_string(),
            criterion_index: 0,
            reason: "impossible".to_string(),
            evidence_commit: None,
            failure_output: None,
            fix_attempts_at_dispute: 1,
            created_at: Utc::now(),
        };
        let yaml = serde_yaml::to_string(&request).unwrap();
        std::fs::write(path, format!("---\n{yaml}---\n\n# Dispute\n")).unwrap();
    }

    /// Relay one request through the relay hook's own writer.
    pub fn relay(&self, record: &Session, kind: RequestKind, payload: Value) -> InboxEntry {
        let entry = entry_for(record, kind, payload);
        let outcome = write_entry(&self.work_dir, &entry).unwrap();
        assert!(matches!(outcome, WriteOutcome::Written));
        entry
    }

    /// Put a file straight into `sid`'s inbox, bypassing the writer's checks.
    pub fn plant(&self, sid: &str, name: &str, bytes: &[u8]) -> PathBuf {
        let dir = self.inbox(sid);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    pub fn inbox(&self, sid: &str) -> PathBuf {
        self.work_dir.join("inbox").join(sid)
    }

    pub fn ledger(&self, sid: &str) -> Vec<LedgerRecord> {
        read_ledger(&self.work_dir, sid).unwrap()
    }

    /// The latest settled outcome the ledger holds for `id`.
    pub fn outcome(&self, sid: &str, id: &str) -> Option<LedgerOutcome> {
        self.ledger(sid)
            .into_iter()
            .rev()
            .find(|record| record.id == id && record.outcome.is_some())
            .and_then(|record| record.outcome)
    }

    pub fn journal_len(&self) -> usize {
        crate::fs::memory::read_journal(&self.work_dir, STAGE)
            .unwrap()
            .entries
            .len()
    }

    pub fn handoff_written(&self) -> bool {
        self.work_dir
            .join("handoffs")
            .join(format!("{STAGE}-handoff-001.md"))
            .exists()
    }
}

/// An entry the relay hook would write for `record`.
pub(super) fn entry_for(record: &Session, kind: RequestKind, payload: Value) -> InboxEntry {
    InboxEntry {
        v: 1,
        id: new_request_id(),
        kind,
        relayed_at: Utc::now(),
        session_id: record.id.clone(),
        stage_id: STAGE.to_string(),
        agent: AgentRole::Main,
        tool_use_id: None,
        payload,
    }
}

pub(super) fn memory_payload(text: &str) -> Value {
    serde_json::to_value(MemoryEntry::new(MemoryEntryType::Note, text.to_string())).unwrap()
}

/// A well-formed payload for each kind.
pub(super) fn payload_for(kind: RequestKind) -> Value {
    match kind {
        RequestKind::Memory => memory_payload("found a pattern"),
        RequestKind::Block => json!({"request": "block", "reason": "stuck on a dependency"}),
        RequestKind::Dispute => json!({
            "request": "dispute",
            "criterion_index": 0,
            "reason": "the criterion cannot pass",
        }),
        RequestKind::Handoff => json!({"trigger": "ceiling"}),
        RequestKind::MergeResolved => json!({}),
        RequestKind::Verdict => json!({"dispute_id": 1, "verdict": REJECT}),
        RequestKind::Telemetry => json!({
            "kind": "context-pulled",
            "stage_id": null,
            "session_id": null,
            "query_chars": 10,
            "budget_tokens": 100,
            "items": 1,
            "estimated_tokens": 20,
            "unmet_required": 0,
        }),
    }
}
