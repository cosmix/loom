//! Reads the optional hook-side ledgers that add spawn metadata the Claude
//! transcript format does not carry. Every lookup is deliberately best-effort:
//! this read-only command remains useful before hooks have created a ledger.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Return the agent type recorded for `agent_id`, if the hook-side ledgers
/// can establish one without guessing. Stage ids are unknown to this command,
/// so every stage directory is considered just as termination lookup does.
pub(super) fn agent_type(work_dir: Option<&Path>, agent_id: &str) -> Option<String> {
    let stage_dirs = stage_directories(work_dir)?;
    match starts_agent_type(&stage_dirs, agent_id) {
        Some(agent_type) => agent_type,
        None => spawns_agent_type(&stage_dirs),
    }
}

fn stage_directories(work_dir: Option<&Path>) -> Option<Vec<PathBuf>> {
    let work_dir = work_dir?;
    let entries = fs::read_dir(work_dir.join("subagents")).ok()?;
    Some(
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect(),
    )
}

/// `Some(None)` means a start row identified the agent but did not carry a
/// usable type. That is still not license to use the unrelated spawn fallback.
fn starts_agent_type(stage_dirs: &[PathBuf], agent_id: &str) -> Option<Option<String>> {
    for stage_dir in stage_dirs {
        for entry in json_lines(&stage_dir.join("starts.jsonl")) {
            if entry.get("agent_id").and_then(Value::as_str) == Some(agent_id) {
                return Some(
                    entry
                        .get("agent_type")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                );
            }
        }
    }
    None
}

/// A spawn ledger predates the agent id, so it is not a per-agent lookup.
/// It is only safe fallback evidence when every usable row across the stages
/// we must scan agrees on one type; any disagreement means the owner cannot
/// be inferred and must remain unknown.
fn spawns_agent_type(stage_dirs: &[PathBuf]) -> Option<String> {
    let mut agreed: Option<String> = None;
    for stage_dir in stage_dirs {
        for entry in json_lines(&stage_dir.join("spawns.jsonl")) {
            let Some(agent_type) = entry.get("agent_type").and_then(Value::as_str) else {
                continue;
            };
            if agreed.as_deref().is_some_and(|known| known != agent_type) {
                return None;
            }
            agreed.get_or_insert_with(|| agent_type.to_string());
        }
    }
    agreed
}

/// Parse independently so a hook appending a partial line cannot hide the
/// valid ledger records before it. Missing or unreadable ledgers simply have
/// no usable rows.
fn json_lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .ok()
        .map(|content| {
            content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_ledger_resolves_matching_agent_type() {
        let work_dir = tempfile::tempdir().unwrap();
        let stage = work_dir.path().join("subagents").join("stage-a");
        fs::create_dir_all(&stage).unwrap();
        fs::write(
            stage.join("starts.jsonl"),
            "not json\n{\"agent_id\":\"agent-x\",\"agent_type\":\"review\"}\n",
        )
        .unwrap();

        assert_eq!(
            agent_type(Some(work_dir.path()), "agent-x"),
            Some("review".into())
        );
    }

    #[test]
    fn matching_spawns_rows_supply_the_safe_fallback() {
        let work_dir = tempfile::tempdir().unwrap();
        let stage = work_dir.path().join("subagents").join("stage-a");
        fs::create_dir_all(&stage).unwrap();
        fs::write(
            stage.join("spawns.jsonl"),
            concat!(
                "{\"agent_type\":\"general-purpose\"}\n",
                "{\"agent_type\":\"general-purpose\"}\n"
            ),
        )
        .unwrap();

        assert_eq!(
            agent_type(Some(work_dir.path()), "agent-without-start"),
            Some("general-purpose".into())
        );
    }

    #[test]
    fn conflicting_spawns_rows_remain_unknown() {
        let work_dir = tempfile::tempdir().unwrap();
        let stage = work_dir.path().join("subagents").join("stage-a");
        fs::create_dir_all(&stage).unwrap();
        fs::write(
            stage.join("spawns.jsonl"),
            concat!(
                "{\"agent_type\":\"review\"}\n",
                "{\"agent_type\":\"implement\"}\n"
            ),
        )
        .unwrap();

        assert_eq!(
            agent_type(Some(work_dir.path()), "agent-without-start"),
            None
        );
    }
}
