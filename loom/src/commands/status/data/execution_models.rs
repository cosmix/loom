//! Reads `spawns.jsonl`, which records spawned agents, and `codex.jsonl`, which
//! records forwarded Codex executions, to collect a stage's execution models.
//! Forwarder rows are skipped because they record the forwarding shim's tier,
//! while the Codex ledger records the model that performed the work.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::fs::work_dir::WorkDir;

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
    append_models(
        &stage_dir.join("codex.jsonl"),
        false,
        &mut seen,
        &mut models,
    );
    models
}

fn valid_stage_id(stage_id: &str) -> bool {
    !stage_id.is_empty()
        && !stage_id.contains("..")
        && stage_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
}

fn append_models(
    path: &Path,
    skip_forwarders: bool,
    seen: &mut HashSet<String>,
    models: &mut Vec<String>,
) {
    for row in json_lines(path) {
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
        let model = model.trim();
        if model.is_empty() {
            continue;
        }

        let display_name = normalize_model(model);
        if seen.insert(display_name.clone()) {
            models.push(display_name);
        }
    }
}

fn json_lines(path: &Path) -> Vec<Value> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };

    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

fn normalize_model(model: &str) -> String {
    let display_name = model.strip_prefix("claude-").unwrap_or(model);
    let display_name = strip_date_suffix(display_name);

    match display_name.strip_prefix("gpt-5.6-") {
        Some(name) => name.to_string(),
        None => display_name.to_string(),
    }
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
            ["sonnet", "opus", "terra", "luna"]
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
            ["haiku-4-5", "sonnet", "terra"]
        );
    }

    #[test]
    fn unsafe_stage_ids_contribute_nothing() {
        let (_temp, work_dir) = test_work_dir();

        assert!(execution_models_for_stage(&work_dir, "bad/stage").is_empty());
        assert!(execution_models_for_stage(&work_dir, "..").is_empty());
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
