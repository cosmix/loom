//! Reads `spawns.jsonl` and correlated Codex lifecycle evidence to collect a
//! stage's execution models. Codex names are explicitly requested-model
//! attribution; companion v1.0.6 does not provide a provider-observed model.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use super::sanitize::valid_stage_id;
use crate::context::untrusted::inline_safe;
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

    let stage_dir = work_dir.root().join("subagents").join(stage_id);
    let mut models = Vec::new();
    let mut seen = HashSet::new();
    append_models(
        &stage_dir.join("spawns.jsonl"),
        true,
        &mut seen,
        &mut models,
    );
    append_codex_models(
        work_dir.root(),
        stage_id,
        &stage_dir.join("codex.jsonl"),
        &mut seen,
        &mut models,
    );
    models
}

fn append_codex_models(
    work_dir: &Path,
    stage_id: &str,
    path: &Path,
    seen: &mut HashSet<String>,
    models: &mut Vec<String>,
) {
    for row in json_lines(path) {
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
    path: &Path,
    skip_forwarders: bool,
    seen: &mut HashSet<String>,
    models: &mut Vec<String>,
) {
    for row in json_lines(path) {
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
        // from `strip_date_suffix`.
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
fn json_lines(path: &Path) -> Vec<Value> {
    let Ok(file) = fs::File::open(path) else {
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
    let display_name = model.strip_prefix("claude-").unwrap_or(model);
    strip_date_suffix(display_name).to_string()
}

/// Strip a trailing `-YYYYMMDD` date stamp. `rsplit_once` splits on a char,
/// so this can never land inside a multi-byte character the way byte-index
/// arithmetic can.
fn strip_date_suffix(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((head, tail))
            if !head.is_empty() && tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) =>
        {
            head
        }
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::execution_models_for_stage;
    use crate::fs::work_dir::WorkDir;

    fn test_work_dir() -> (tempfile::TempDir, WorkDir) {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = WorkDir::new(temp.path().join(".loom/work")).unwrap();
        (temp, work_dir)
    }

    #[test]
    fn forwarder_rows_are_skipped_and_codex_rows_counted() {
        let (_temp, work_dir) = test_work_dir();
        let stage_dir = work_dir.root().join("subagents").join("s1");
        std::fs::create_dir_all(&stage_dir).unwrap();
        std::fs::write(
            stage_dir.join("spawns.jsonl"),
            concat!(
                r#"{"agent_type":"loom-software-engineer","model":"sonnet"}"#,
                "\n",
                r#"{"agent_type":"loom-codex-forwarder","model":"sonnet"}"#,
                "\n",
                r#"{"agent_type":"loom-senior-software-engineer","model":"opus"}"#,
                "\n",
            ),
        )
        .unwrap();
        std::fs::write(
            stage_dir.join("codex.jsonl"),
            concat!(
                r#"{"model":"gpt-5.6-terra"}"#,
                "\n",
                r#"{"model":"gpt-5.6-terra"}"#,
                "\n",
                r#"{"model":"gpt-5.6-luna"}"#,
                "\n",
            ),
        )
        .unwrap();

        assert_eq!(
            execution_models_for_stage(&work_dir, "s1"),
            [
                "sonnet",
                "opus",
                "gpt-5.6-terra (requested)",
                "gpt-5.6-luna (requested)"
            ]
        );
    }

    #[test]
    fn codex_model_family_and_future_names_are_preserved() {
        let (_temp, work_dir) = test_work_dir();
        let stage_dir = work_dir.root().join("subagents").join("s1");
        std::fs::create_dir_all(&stage_dir).unwrap();
        std::fs::write(
            stage_dir.join("codex.jsonl"),
            concat!(
                r#"{"model":"gpt-5.6-sol"}"#,
                "\n",
                r#"{"model":"gpt-6-astra"}"#,
                "\n",
                r#"{"model":"gpt-7-orbit"}"#,
                "\n",
                r#"{"v":2,"model":"gpt-unmatched"}"#,
                "\n",
            ),
        )
        .unwrap();

        assert_eq!(
            execution_models_for_stage(&work_dir, "s1"),
            [
                "gpt-5.6-sol (requested)",
                "gpt-6-astra (requested)",
                "gpt-7-orbit (requested)"
            ]
        );
    }

    #[test]
    fn missing_stage_directory_contributes_nothing() {
        let (_temp, work_dir) = test_work_dir();

        assert!(execution_models_for_stage(&work_dir, "s1").is_empty());
    }

    #[test]
    fn blank_and_malformed_rows_are_skipped() {
        let (_temp, work_dir) = test_work_dir();
        let stage_dir = work_dir.root().join("subagents").join("s1");
        std::fs::create_dir_all(&stage_dir).unwrap();
        std::fs::write(
            stage_dir.join("spawns.jsonl"),
            "\nnot json\n{\"model\":\"claude-haiku-4-5-20251001\"}\n\n{\"model\":\"sonnet\"}\n",
        )
        .unwrap();
        std::fs::write(
            stage_dir.join("codex.jsonl"),
            "not json\n{\"model\":\"gpt-5.6-terra\"}\n",
        )
        .unwrap();

        assert_eq!(
            execution_models_for_stage(&work_dir, "s1"),
            ["haiku-4-5", "sonnet", "gpt-5.6-terra (requested)"]
        );
    }

    #[test]
    fn unsafe_stage_ids_contribute_nothing() {
        let (_temp, work_dir) = test_work_dir();

        assert!(execution_models_for_stage(&work_dir, "bad/stage").is_empty());
        assert!(execution_models_for_stage(&work_dir, ".").is_empty());
        assert!(execution_models_for_stage(&work_dir, "..").is_empty());
    }

    #[test]
    fn names_differing_only_in_invisible_characters_are_one_model() {
        let (_temp, work_dir) = test_work_dir();
        let stage_dir = work_dir.root().join("subagents").join("s1");
        std::fs::create_dir_all(&stage_dir).unwrap();
        std::fs::write(
            stage_dir.join("spawns.jsonl"),
            concat!(
                r#"{"model":"sonnet\u200b"}"#,
                "\n",
                r#"{"model":"sonnet "}"#,
                "\n",
                r#"{"model":"   "}"#,
                "\n",
                r#"{"model":"claude-haiku-4-5-20251001\u200b"}"#,
                "\n",
            ),
        )
        .unwrap();

        assert_eq!(
            execution_models_for_stage(&work_dir, "s1"),
            ["sonnet", "haiku-4-5"]
        );
    }

    #[test]
    fn a_multibyte_model_name_does_not_panic() {
        let (_temp, work_dir) = test_work_dir();
        let stage_dir = work_dir.root().join("subagents").join("s1");
        std::fs::create_dir_all(&stage_dir).unwrap();
        std::fs::write(
            stage_dir.join("spawns.jsonl"),
            concat!(
                r#"{"model":"aãaaaaaaaa"}"#,
                "\n",
                r#"{"model":"claude-haiku-4-5-20251001"}"#,
                "\n",
            ),
        )
        .unwrap();

        assert_eq!(
            execution_models_for_stage(&work_dir, "s1"),
            ["aãaaaaaaaa", "haiku-4-5"]
        );
    }
}
