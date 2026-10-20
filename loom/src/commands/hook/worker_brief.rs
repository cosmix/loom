//! Scoped retrieval briefs for typed Task/Agent workers, each filed as a delivery record when it is issued.

use crate::context::config::RetrievalConfig;
use crate::context::delivery::{self, DeliveryRecord};
use crate::context::rank_source::normalize_dependency_path;
use crate::context::retrieve::{retrieve_for_stage, StageQuery, Surface};
use crate::context::schema::{ContextItem, ContextPack, UnmetRequirement};
use crate::fs::locking::{atomic_write_locked, locked_dir_update};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use crate::orchestrator::signals::format_knowledge_brief;
use crate::validation::validate_id;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::target::non_empty_env;

const MAX_STDIN_BYTES: u64 = 1024 * 1024;
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

/// Print one worker-brief envelope for the spawn payload on stdin, or `{}`.
pub fn worker_brief() -> Result<()> {
    emit_normal();
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

/// Maximum worker retrieval-query size. Retrieval takes about 1.7 seconds at
/// 2,048 bytes but 9.6 seconds for a recorded loom spawn whose query carried
/// an 11 KB stage description, against the spawn guard's former three-second
/// bound. `spawn-guard.sh` gives this delegate five seconds; revisit this cap
/// once retrieval cost stops growing with query length.
pub const WORKER_QUERY_MAX_BYTES: usize = 2_048;

/// Build a worker-first retrieval query. The worker's own task leads because
/// stage text already reaches the stage session through its signal's brief.
/// When description plus prompt reach 2,048 bytes, the stage text is cut
/// entirely. A recorded loom spawn prompt is 3,580 bytes, so loom's own stages
/// query the worker description and prompt. `StageQuery::build_stage_query_text`
/// supplies the stage text.
pub fn worker_query_text(stage: &Stage, description: &str, prompt: &str) -> String {
    let stage_text = StageQuery::build_stage_query_text(stage);
    let text = [description, prompt, &stage_text].join("\n");
    if text.len() <= WORKER_QUERY_MAX_BYTES {
        return text;
    }
    text[..text.floor_char_boundary(WORKER_QUERY_MAX_BYTES)].to_string()
}

fn issue(raw: &str, config: &Config) -> Option<Envelope> {
    let payload: HookPayload = serde_json::from_str(raw).ok()?;
    if !valid_payload(&payload) {
        return None;
    }
    let paths = declared_paths(&payload.tool_input.prompt);
    let pack = retrieve_pack(config, &payload, &paths)?;
    // Declared skills must reach the worker even with an empty context pack.
    if pack.items.is_empty() && pack.unmet_required.is_empty() && config.stage.skills.is_empty() {
        return None;
    }

    let nonce = Uuid::new_v4().simple().to_string();
    let brief = render_brief(&pack, &config.stage, &nonce)?;
    let envelope = Envelope { nonce, brief };
    let encoded = serde_json::to_string(&envelope).ok()?;
    if encoded.len() > config.retrieval.max_payload_bytes {
        return None;
    }
    store_record(config, &envelope.nonce, &pack).ok()??;
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
    let tool = &payload.tool_input;
    let text = worker_query_text(&config.stage, &tool.description, &tool.prompt);
    let mut query = StageQuery::new(&config.work_dir, text).with_surface(Surface::Hook);
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

fn render_brief(pack: &ContextPack, stage: &Stage, nonce: &str) -> Option<String> {
    let rendered = format_knowledge_brief(pack, Some(&stage.id), QUERY_INPUTS);
    if rendered.lines().any(|line| marker_nonce(line).is_some()) {
        return None;
    }
    let mut brief = format!("<!-- loom-worker-brief nonce={nonce} -->\n{rendered}");
    if let Some(line) = skills_line(&stage.skills) {
        brief.push_str(&line);
    }
    Some(brief)
}

/// Names the stage's plan-declared skills and how to load each; `None` when it declares none.
fn skills_line(skills: &[String]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let calls: Vec<String> = skills
        .iter()
        .map(|name| crate::skills::skill_invocation(name))
        .collect();
    Some(format!(
        "\nRequired skills for this stage — invoke: {}\n",
        calls.join(", ")
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

/// File a delivery record when this brief is issued. The brief rides the
/// spawn's own `updatedInput`, so a delegate killed before stdout leaves a
/// record for a brief it never carried. That is accepted because a delivery
/// record is an optimisation (`context::delivery` module doc).
fn store_record(config: &Config, nonce: &str, pack: &ContextPack) -> Result<Option<()>> {
    let dir = delivery::worker_delivery_dir(
        &config.work_dir,
        delivery::plan_key(&config.stage),
        &config.stage.id,
    );
    let key = delivery::worker_recipient_id(&config.stage.id, &config.parent_session, nonce);
    let bound = dir.join(format!("{key}.json"));
    let record = DeliveryRecord::from_pack(key, pack);
    let json = serde_json::to_string_pretty(&record)?;
    locked_dir_update(&dir, || {
        if bound.exists() {
            return Ok(None);
        }
        atomic_write_locked(&bound, &json)?;
        Ok(Some(()))
    })
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
