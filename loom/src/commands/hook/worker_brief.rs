//! Scoped retrieval briefs for typed Task/Agent workers.

use crate::context::config::RetrievalConfig;
use crate::context::delivery::{self, DeliveryRecord};
use crate::context::rank_source::normalize_dependency_path;
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::schema::{ContextItem, ContextPack, UnmetRequirement};
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use crate::orchestrator::signals::format_knowledge_brief;
use crate::validation::{validate_id, MAX_ID_LENGTH};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::target::non_empty_env;

const MAX_STDIN_BYTES: u64 = 1024 * 1024;
const MAX_TRANSCRIPT_BYTES: u64 = 1024 * 1024;
const QUERY_INPUTS: &str = "the stage inputs, worker task and explicitly declared paths";
const CODEX_FORWARD_PATH: &str = "loom-hooks/codex-forward.sh";

#[derive(Deserialize)]
struct HookPayload {
    tool_name: String,
    tool_input: ToolInput,
    session_id: String,
    cwd: PathBuf,
    transcript_path: PathBuf,
}

#[derive(Deserialize)]
struct ToolInput {
    subagent_type: String,
    prompt: String,
    description: String,
}

#[derive(Serialize)]
struct Envelope {
    nonce: String,
    brief: String,
}

struct Config {
    work_dir: PathBuf,
    stage: Stage,
    parent_session: String,
    retrieval: RetrievalConfig,
}

impl Config {
    fn from_env() -> Option<Self> {
        let stage_id = non_empty_env("LOOM_STAGE_ID")?;
        validate_id(&stage_id).ok()?;
        let resolved = WorkDir::new(non_empty_env("LOOM_WORK_DIR")?).ok()?;
        let work_dir = resolved.root().to_path_buf();
        if !work_dir.is_dir() {
            return None;
        }
        let stage = crate::verify::load_stage(&stage_id, &work_dir).ok()?;
        let parent_session = non_empty_env("LOOM_SESSION_ID")?;
        let main_root = resolved
            .main_project_root()
            .unwrap_or_else(|| work_dir.clone());
        let retrieval = RetrievalConfig::load(&main_root);
        Some(Self {
            work_dir,
            stage,
            parent_session,
            retrieval,
        })
    }
}

/// Run normal emission or bind one issued nonce to a started child.
pub fn worker_brief(
    bind_agent: Option<String>,
    agent_type: Option<String>,
    transcript: Option<PathBuf>,
) -> Result<()> {
    match (
        bind_agent.as_deref(),
        agent_type.as_deref(),
        transcript.as_deref(),
    ) {
        (None, None, None) => emit_normal(),
        (Some(agent), Some(kind), Some(path)) => {
            if let Some(config) = Config::from_env() {
                let _ = bind_pending(&config, agent, kind, path);
            }
        }
        _ => {}
    }
    Ok(())
}

fn emit_normal() {
    let envelope = read_stdin()
        .and_then(|raw| Config::from_env().map(|config| issue_line(&raw, &config)))
        .unwrap_or_else(|| "{}".to_string());
    let _ = writeln!(std::io::stdout().lock(), "{envelope}");
}

fn issue_line(raw: &str, config: &Config) -> String {
    issue(raw, config)
        .and_then(|value| serde_json::to_string(&value).ok())
        .unwrap_or_else(|| "{}".to_string())
}

fn read_stdin() -> Option<String> {
    let mut raw = String::new();
    std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES + 1)
        .read_to_string(&mut raw)
        .ok()?;
    (raw.len() <= MAX_STDIN_BYTES as usize).then_some(raw)
}

fn issue(raw: &str, config: &Config) -> Option<Envelope> {
    let payload: HookPayload = serde_json::from_str(raw).ok()?;
    if !valid_payload(&payload) {
        return None;
    }
    let paths = declared_paths(&payload.tool_input.prompt);
    let pack = retrieve_pack(config, &payload, &paths)?;
    if pack.items.is_empty() && pack.unmet_required.is_empty() {
        return None;
    }

    let nonce = Uuid::new_v4().simple().to_string();
    let brief = render_brief(&pack, &config.stage.id, &nonce)?;
    let envelope = Envelope { nonce, brief };
    let encoded = serde_json::to_string(&envelope).ok()?;
    if encoded.len() > config.retrieval.max_payload_bytes {
        return None;
    }
    store_pending(config, &envelope.nonce, &pack).ok()??;
    Some(envelope)
}

fn valid_payload(payload: &HookPayload) -> bool {
    matches!(payload.tool_name.as_str(), "Task" | "Agent")
        && !payload.tool_input.subagent_type.trim().is_empty()
        && !payload.tool_input.prompt.trim().is_empty()
        && !payload.tool_input.description.trim().is_empty()
        && !payload.session_id.trim().is_empty()
        && !payload.cwd.as_os_str().is_empty()
        && !payload.transcript_path.as_os_str().is_empty()
}

fn retrieve_pack(config: &Config, payload: &HookPayload, paths: &[String]) -> Option<ContextPack> {
    let mut text = StageQuery::build_stage_query_text(&config.stage);
    text.push('\n');
    text.push_str(&payload.tool_input.description);
    text.push('\n');
    text.push_str(&payload.tool_input.prompt);
    let mut query = StageQuery::new(&config.work_dir, text);
    query.overlay = StageQuery::stage_overlay_scope(&config.stage);
    query.stage_dependency_ids = delivery::dependency_chunk_ids(
        &config.work_dir,
        delivery::plan_key(&config.stage),
        &config.stage.dependencies,
    );
    query.dependency_paths = StageQuery::dependency_paths(&config.work_dir, &config.stage);
    for path in paths {
        if !query.dependency_paths.contains(path) {
            query.dependency_paths.push(path.clone());
        }
    }
    let mut pack = retrieve_for_stage(&query, config.retrieval.stage_brief_budget_tokens).ok()?;
    scope_pack(&mut pack, &query.required_ids);
    Some(pack)
}

fn scope_pack(pack: &mut ContextPack, required_ids: &[String]) {
    let mut scoped = Vec::with_capacity(pack.items.len());
    for item in std::mem::take(&mut pack.items) {
        if worker_material(&item) {
            scoped.push(item);
            continue;
        }
        pack.omitted.omitted += 1;
        if required_ids.iter().any(|id| id == item.id.as_str()) {
            pack.unmet_required.push(UnmetRequirement {
                id: item.id.as_str().to_string(),
                needed_tokens: item.token_count,
                available_tokens: 0,
                reason: "required item excluded from worker brief scope".to_string(),
            });
        }
    }
    pack.items = scoped;
    pack.recompute_estimate();
}

fn worker_material(item: &ContextItem) -> bool {
    let path = normalize_dependency_path(&item.pointer.path.display().to_string());
    !plan_document(&path) && path != CODEX_FORWARD_PATH
}

fn plan_document(path: &str) -> bool {
    path.strip_prefix("doc/plans/")
        .is_some_and(|name| !name.contains('/') && name.contains("PLAN-"))
}

fn render_brief(pack: &ContextPack, stage: &str, nonce: &str) -> Option<String> {
    let rendered = format_knowledge_brief(pack, Some(stage), QUERY_INPUTS);
    if rendered.lines().any(|line| marker_nonce(line).is_some()) {
        return None;
    }
    Some(format!(
        "<!-- loom-worker-brief nonce={nonce} -->\n{rendered}"
    ))
}

fn declared_paths(prompt: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_paths = false;
    for line in prompt.lines() {
        let trimmed = line.trim();
        if path_heading(trimmed) {
            in_paths = true;
            continue;
        }
        if trimmed.starts_with('#') || (in_paths && !trimmed.is_empty() && !is_bullet(trimmed)) {
            in_paths = false;
        }
        if in_paths {
            if let Some(path) = bullet_path(trimmed) {
                if !paths.contains(&path) {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

fn path_heading(line: &str) -> bool {
    let heading = line
        .trim_start_matches('#')
        .trim()
        .trim_matches('*')
        .trim_end_matches(':')
        .to_ascii_lowercase();
    heading.starts_with("files owned")
        || heading.starts_with("files may read")
        || heading.starts_with("may read")
}

fn is_bullet(line: &str) -> bool {
    line.starts_with("- ") || line.starts_with("* ")
}

fn bullet_path(line: &str) -> Option<String> {
    let value = line.get(2..)?.trim().trim_matches('`').trim_matches('*');
    let value = value.split_whitespace().next()?.trim_matches('`');
    let path = normalize_dependency_path(value);
    if path.is_empty()
        || path.len() > 512
        || Path::new(&path).is_absolute()
        || Path::new(&path)
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(path)
}

fn store_pending(config: &Config, nonce: &str, pack: &ContextPack) -> Result<Option<()>> {
    let dir = delivery::worker_delivery_dir(
        &config.work_dir,
        delivery::plan_key(&config.stage),
        &config.stage.id,
    );
    let key = delivery::worker_recipient_id(&config.stage.id, &config.parent_session, nonce);
    let pending = dir.join(format!("{key}.pending"));
    let bound = dir.join(format!("{key}.json"));
    let record = DeliveryRecord::from_pack(key, pack);
    let json = serde_json::to_string_pretty(&record)?;
    locked_dir_update(&dir, || {
        if pending.exists() || bound.exists() {
            return Ok(None);
        }
        atomic_write_locked(&pending, &json)?;
        Ok(Some(()))
    })
}

fn bind_pending(config: &Config, agent: &str, _agent_type: &str, transcript: &Path) -> bool {
    if agent.trim().is_empty() || agent.len() > MAX_ID_LENGTH {
        return false;
    }
    let Some(nonce) = read_transcript(transcript).and_then(|raw| transcript_nonce(&raw)) else {
        return false;
    };
    bind_nonce(config, agent, &nonce).unwrap_or(false)
}

fn bind_nonce(config: &Config, agent: &str, nonce: &str) -> Result<bool> {
    let dir = delivery::worker_delivery_dir(
        &config.work_dir,
        delivery::plan_key(&config.stage),
        &config.stage.id,
    );
    let key = delivery::worker_recipient_id(&config.stage.id, &config.parent_session, nonce);
    let pending = dir.join(format!("{key}.pending"));
    let bound = dir.join(format!("{key}.json"));
    locked_dir_update(&dir, || {
        if bound.exists() || !pending.is_file() {
            return Ok(false);
        }
        let raw = fs::read_to_string(&pending).context("reading pending worker brief")?;
        let mut record: DeliveryRecord = serde_json::from_str(&raw)?;
        record.recipient_id = agent.to_string();
        let json = serde_json::to_string_pretty(&record)?;
        atomic_write_locked(&bound, &json)?;
        fs::remove_file(&pending).context("retiring pending worker brief")?;
        Ok(true)
    })
}

fn read_transcript(path: &Path) -> Option<String> {
    crate::models::forward_receipt::locator::read_bounded_prefix(
        path,
        MAX_TRANSCRIPT_BYTES as usize,
    )
    .ok()
}

fn transcript_nonce(raw: &str) -> Option<String> {
    let mut found = BTreeSet::new();
    for line in raw.lines() {
        if let Some(nonce) = marker_nonce(line) {
            found.insert(nonce);
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            collect_value_markers(&value, &mut found);
        }
    }
    (found.len() == 1)
        .then(|| found.into_iter().next())
        .flatten()
}

fn collect_value_markers(value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::String(text) => found.extend(text.lines().filter_map(marker_nonce)),
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_value_markers(value, found)),
        Value::Object(values) => values
            .values()
            .for_each(|value| collect_value_markers(value, found)),
        _ => {}
    }
}

fn marker_nonce(line: &str) -> Option<String> {
    let nonce = line
        .trim()
        .strip_prefix("<!-- loom-worker-brief nonce=")?
        .strip_suffix(" -->")?;
    (nonce.len() == 32
        && nonce
            .chars()
            .all(|ch| ch.is_ascii_digit() || matches!(ch, 'a'..='f')))
    .then(|| nonce.to_string())
}

#[cfg(test)]
#[path = "tests_worker_brief.rs"]
mod tests;
