//! Reads `spawns.jsonl` and correlated Codex lifecycle evidence to collect a
//! stage's execution models. Codex names are explicitly requested-model
//! attribution; companion v1.0.6 does not provide a provider-observed model.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use super::sanitize::valid_stage_id;
use crate::context::untrusted::inline_safe;
use crate::fs::safe_read::open_regular_no_follow;
use crate::fs::work_dir::WorkDir;

/// Most of one ledger the collector reads.
///
/// The collector re-reads both ledgers for every stage once a second, so an
/// unbounded read is an unbounded per-second allocation. A spawn row is a few
/// hundred bytes, so 256 KiB covers several hundred subagents on a single
/// stage — far past anything a real stage records — while capping the read.
const MAX_LEDGER_BYTES: u64 = 256 * 1024;

/// Most distinct model names kept for one stage.
///
/// A stage runs a handful of tiers; there are fewer than eight model names in
/// the whole allocation table. Past that the list has stopped identifying who
/// did the work and started being a payload a ledger file's author controls.
const MAX_EXECUTION_MODELS: usize = 8;

/// Return distinct execution model display names recorded for one stage.
pub fn execution_models_for_stage(work_dir: &WorkDir, stage_id: &str) -> Vec<String> {
    if !valid_stage_id(stage_id) {
        return Vec::new();
    }

    // A linked worktree can use a trusted symlink for `.loom/work`; every
    // ledger beneath its resolved root is still opened without following links.
    let Ok(root) = std::fs::canonicalize(work_dir.root()) else {
        return Vec::new();
    };
    let stage_dir = root.join("subagents").join(stage_id);
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    append_models(
        &root,
        &stage_dir.join("spawns.jsonl"),
        true,
        &mut seen,
        &mut models,
    );
    append_codex_models(
        &root,
        stage_id,
        &stage_dir.join("codex.jsonl"),
        &mut seen,
        &mut models,
    );
    for thread in crate::native_worker::index::stage_threads(&root, stage_id) {
        let name = format!("{} (requested)", normalize_model(&thread.requested_model));
        if models.len() < MAX_EXECUTION_MODELS {
            append_model(&name, &mut seen, &mut models);
        }
    }
    models
}

fn append_codex_models(
    work_dir: &Path,
    stage_id: &str,
    path: &Path,
    seen: &mut HashSet<String>,
    models: &mut Vec<String>,
) {
    for row in json_lines(work_dir, path) {
        if models.len() >= MAX_EXECUTION_MODELS {
            return;
        }
        let model = if row.get("v").and_then(Value::as_u64) == Some(2) {
            correlated_requested_model(work_dir, stage_id, &row)
        } else {
            row.get("model")
                .and_then(Value::as_str)
                .filter(|model| (1..=128).contains(&model.len()) && !model.as_bytes().contains(&0))
                .map(str::to_owned)
        };
        let Some(model) = model else {
            continue;
        };
        let normalized = normalize_model(&inline_safe(&model));
        if !normalized.is_empty() {
            append_model(&format!("{normalized} (requested)"), seen, models);
        }
    }
}

fn correlated_requested_model(work_dir: &Path, stage_id: &str, row: &Value) -> Option<String> {
    let authorization = crate::codex_lifecycle::CodexAuthorization::from_v2_value(row).ok()?;
    if authorization.stage_id != stage_id
        || !crate::codex_lifecycle::has_correlated_lifecycle(work_dir, &authorization)
    {
        return None;
    }
    Some(authorization.model)
}

fn append_models(
    root: &Path,
    path: &Path,
    skip_forwarders: bool,
    seen: &mut HashSet<String>,
    models: &mut Vec<String>,
) {
    for row in json_lines(root, path) {
        if models.len() >= MAX_EXECUTION_MODELS {
            return;
        }

        if skip_forwarders
            && row
                .get("agent_type")
                .and_then(Value::as_str)
                .is_some_and(|agent_type| agent_type == "loom-codex-forwarder")
        {
            continue;
        }

        let Some(model) = row.get("model").and_then(Value::as_str) else {
            continue;
        };

        // Flatten BEFORE the dedup key is taken. The renderers only ever show
        // the flattened form, so `"sonnet\u{200B}"` and `"sonnet "` are one
        // display name; deduping on the raw name let both through and drew two
        // rows reading `sonnet`. Flattening first also trims, and keeps a
        // trailing zero-width character from hiding a `-YYYYMMDD` date stamp
        // from model-name normalization.
        append_model(&normalize_model(&inline_safe(model)), seen, models);
    }
}

fn append_model(display_name: &str, seen: &mut HashSet<String>, models: &mut Vec<String>) {
    if !display_name.is_empty() && seen.insert(display_name.into()) {
        models.push(display_name.into());
    }
}

/// Parse the rows of a JSONL ledger, reading at most [`MAX_LEDGER_BYTES`].
///
/// A cap truncates the last row rather than the file, and a truncated row
/// fails to parse and is dropped like any other malformed one; the bytes are
/// decoded lossily so a cut multi-byte character costs that row alone and not
/// the whole read.
fn json_lines(root: &Path, path: &Path) -> Vec<Value> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Vec::new();
    };
    let Some(relative) = relative.to_str() else {
        return Vec::new();
    };
    let Ok(Some(file)) = open_regular_no_follow(root, relative, libc::O_RDONLY) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    if file.take(MAX_LEDGER_BYTES).read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }

    String::from_utf8_lossy(&bytes)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn normalize_model(model: &str) -> String {
    crate::commands::common::model_display::normalize_model(model).to_string()
}

#[cfg(test)]
#[path = "execution_models_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "execution_models_native_tests.rs"]
mod native_tests;
