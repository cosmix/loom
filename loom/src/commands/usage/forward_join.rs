use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::provider_types::{
    NormalizedEvent, Provider, ProviderDiagnostics, ProviderLedger, SourceKind,
};
use super::transcript::{Entry, Scope, Transcript};
use crate::models::forward_receipt::{
    is_safe_id, load_receipts, receipts_path, ForwardBackend, ForwardIdentity, ForwardReceipt,
    ForwardState,
};

const MAX_STAGE_RECEIPT_FILES: usize = 4_096;
const FORWARDER_TYPE: &str = "loom-codex-forwarder";
const FORWARDER_SENTINEL: &str = "LOOM-CODEX-FORWARD-ONLY";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ForwardMetadata {
    pub(crate) receipt_id: String,
    pub(crate) backend: ForwardBackend,
    #[serde(skip)]
    pub(crate) backend_id: String,
    pub(crate) state: ForwardState,
}

impl From<&ForwardReceipt> for ForwardMetadata {
    fn from(receipt: &ForwardReceipt) -> Self {
        Self {
            receipt_id: receipt.receipt_id.clone(),
            backend: receipt.backend,
            backend_id: receipt.backend_id.clone(),
            state: receipt.state,
        }
    }
}

enum Candidate {
    Unique(ForwardMetadata),
    Ambiguous,
}

pub(crate) struct ForwardJoin {
    by_receipt_id: HashMap<String, ForwardMetadata>,
    by_thread_id: HashMap<String, Candidate>,
    load_diagnostics: ProviderDiagnostics,
}

impl ForwardJoin {
    pub(crate) fn load(root: Option<&Path>) -> Self {
        let mut load_diagnostics = ProviderDiagnostics::default();
        let receipts = load_root(root, &mut load_diagnostics);
        let mut result = Self {
            by_receipt_id: HashMap::new(),
            by_thread_id: HashMap::new(),
            load_diagnostics,
        };
        for receipt in receipts {
            let metadata = ForwardMetadata::from(&receipt);
            result
                .by_receipt_id
                .insert(receipt.receipt_id.clone(), metadata.clone());
            if let Some(thread_id) = receipt_thread_id(&receipt) {
                insert_candidate(&mut result.by_thread_id, thread_id, metadata);
            }
        }
        result
    }

    pub(crate) fn join_transcript(&self, transcript: &mut Transcript) {
        transcript.forward_candidate = is_forwarder(transcript);
        transcript.forward_receipt = None;
        let Some((agent_id, stage_id, loom_session_id)) = transcript_identity(transcript) else {
            return;
        };
        let mut matches = HashMap::<String, ForwardMetadata>::new();
        for tool_use_id in completed_tool_uses(transcript) {
            let Ok(identity) = ForwardIdentity::new(
                &transcript.session_id,
                agent_id,
                tool_use_id,
                stage_id,
                loom_session_id,
            ) else {
                continue;
            };
            if let Some(metadata) = self.by_receipt_id.get(&identity.receipt_id()) {
                matches.insert(metadata.receipt_id.clone(), metadata.clone());
            }
        }
        transcript.forward_candidate |= !matches.is_empty();
        if matches.len() == 1 {
            transcript.forward_receipt = matches.into_values().next();
        }
    }

    pub(crate) fn join_ledger(&self, ledger: &mut ProviderLedger) {
        let mut claude = ProviderDiagnostics::default();
        let mut codex = ProviderDiagnostics::default();
        for row in &mut ledger.rows {
            let failure = self.join_row(row);
            let diagnostics = match row.provider {
                Provider::Claude => &mut claude,
                Provider::Codex => &mut codex,
            };
            record_failure(diagnostics, failure);
        }
        for report in &mut ledger.providers {
            report.diagnostics.merge(self.load_diagnostics.clone());
            match report.provider {
                Provider::Claude => report.diagnostics.merge(claude.clone()),
                Provider::Codex => report.diagnostics.merge(codex.clone()),
            }
        }
    }

    fn join_row(&self, row: &mut NormalizedEvent) -> JoinFailure {
        if row.provider == Provider::Claude {
            return self.join_claude_row(row);
        }
        if !matches!(
            row.source_kind,
            SourceKind::CodexDirect | SourceKind::CodexFallback
        ) {
            return JoinFailure::None;
        }
        row.forward_receipt = None;
        if row.codex_thread_conflict {
            return JoinFailure::Conflict;
        }
        let Some(thread_id) = row.codex_thread_id.as_deref() else {
            return JoinFailure::Absent;
        };
        match self.by_thread_id.get(thread_id) {
            Some(Candidate::Unique(metadata)) => {
                row.forward_receipt = Some(metadata.clone());
                JoinFailure::None
            }
            Some(Candidate::Ambiguous) => JoinFailure::Ambiguous,
            None => JoinFailure::Absent,
        }
    }

    fn join_claude_row(&self, row: &mut NormalizedEvent) -> JoinFailure {
        let Some(receipt_id) = row
            .forward_receipt
            .as_ref()
            .map(|metadata| metadata.receipt_id.clone())
        else {
            return if row.forward_candidate {
                JoinFailure::Absent
            } else {
                JoinFailure::None
            };
        };
        match self.by_receipt_id.get(&receipt_id) {
            Some(metadata) => {
                row.forward_receipt = Some(metadata.clone());
                JoinFailure::None
            }
            None => {
                row.forward_receipt = None;
                JoinFailure::Absent
            }
        }
    }
}

#[derive(Clone, Copy)]
enum JoinFailure {
    None,
    Absent,
    Ambiguous,
    Conflict,
}

fn record_failure(diagnostics: &mut ProviderDiagnostics, failure: JoinFailure) {
    match failure {
        JoinFailure::None => {}
        JoinFailure::Absent => diagnostics.forward_identity_absent += 1,
        JoinFailure::Ambiguous => diagnostics.forward_identity_ambiguous += 1,
        JoinFailure::Conflict => diagnostics.forward_identity_conflicts += 1,
    }
}

fn insert_candidate(
    candidates: &mut HashMap<String, Candidate>,
    key: String,
    metadata: ForwardMetadata,
) {
    use std::collections::hash_map::Entry;
    match candidates.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(Candidate::Unique(metadata));
        }
        Entry::Occupied(mut entry) => {
            entry.insert(Candidate::Ambiguous);
        }
    }
}

fn receipt_thread_id(receipt: &ForwardReceipt) -> Option<String> {
    match receipt.backend {
        ForwardBackend::Companion => receipt.codex_thread_id.clone(),
        ForwardBackend::Direct => Some(receipt.backend_id.clone()),
    }
}

fn transcript_identity(transcript: &Transcript) -> Option<(&str, &str, &str)> {
    if transcript.scope != Scope::Subagent {
        return None;
    }
    Some((
        transcript.agent_id.as_deref()?,
        transcript.stage_id.as_deref()?,
        transcript.loom_session_id.as_deref()?,
    ))
}

fn is_forwarder(transcript: &Transcript) -> bool {
    transcript.scope == Scope::Subagent
        && (transcript.agent_type.as_deref() == Some(FORWARDER_TYPE)
            || transcript
                .first_user_entry
                .as_ref()
                .is_some_and(|entry| entry.text.contains(FORWARDER_SENTINEL)))
}

fn completed_tool_uses(transcript: &Transcript) -> Vec<&str> {
    let mut pending = HashSet::<&str>::new();
    let mut completed = Vec::new();
    for entry in &transcript.entries {
        match entry {
            Entry::Assistant(request) => {
                pending.extend(request.tool_uses.iter().map(|tool| tool.id.as_str()));
            }
            Entry::User(user) => {
                let Some(tool_use_id) = user.tool_use_id.as_deref() else {
                    continue;
                };
                if pending.remove(tool_use_id) && !completed.contains(&tool_use_id) {
                    completed.push(tool_use_id);
                }
            }
        }
    }
    completed
}

fn load_root(root: Option<&Path>, diagnostics: &mut ProviderDiagnostics) -> Vec<ForwardReceipt> {
    let Some(root) = root else {
        diagnostics.forward_receipt_scope_unavailable += 1;
        return Vec::new();
    };
    let Some(files) = receipt_files(root, diagnostics) else {
        return Vec::new();
    };
    let mut receipts = Vec::new();
    for (stage_id, path) in files {
        diagnostics.forward_receipt_files_seen += 1;
        let loaded = match load_receipts(&path) {
            Ok(loaded) => loaded,
            Err(_) => {
                diagnostics.unreadable_files += 1;
                continue;
            }
        };
        diagnostics.malformed_forward_receipts += loaded.malformed;
        if loaded.truncated {
            diagnostics.truncated_forward_receipt_files += 1;
            continue;
        }
        for receipt in loaded.receipts {
            if receipt.identity.stage_id == stage_id {
                receipts.push(receipt);
            } else {
                diagnostics.malformed_forward_receipts += 1;
            }
        }
    }
    receipts
}

fn receipt_subagent_entries(
    root: &Path,
    diagnostics: &mut ProviderDiagnostics,
) -> Result<Option<fs::ReadDir>, ()> {
    let subagents = root.join("subagents");
    match subagents.symlink_metadata().and_then(|metadata| {
        if metadata.file_type().is_dir() {
            fs::read_dir(&subagents)
        } else {
            Err(std::io::Error::other("subagents is not a directory"))
        }
    }) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => {
            diagnostics.unreadable_files += 1;
            Err(())
        }
    }
}

fn receipt_files(
    root: &Path,
    diagnostics: &mut ProviderDiagnostics,
) -> Option<Vec<(String, PathBuf)>> {
    if !root.is_dir() {
        diagnostics.missing_forward_receipt_roots += 1;
        return None;
    }
    let entries = match receipt_subagent_entries(root, diagnostics) {
        Ok(Some(entries)) => entries,
        Ok(None) => return Some(Vec::new()),
        Err(()) => return None,
    };
    let mut files = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_STAGE_RECEIPT_FILES {
            diagnostics.truncated_forward_receipt_files += 1;
            return None;
        }
        let Ok(entry) = entry else {
            diagnostics.unreadable_files += 1;
            continue;
        };
        let Some(stage_id) = entry.file_name().to_str().map(str::to_owned) else {
            diagnostics.malformed_forward_receipts += 1;
            continue;
        };
        if !is_safe_id(&stage_id) || !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let path = receipts_path(root, &stage_id).ok()?;
        if path
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_file())
        {
            files.push((stage_id, path));
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Some(files)
}

#[cfg(test)]
#[path = "forward_join_tests.rs"]
mod tests;
