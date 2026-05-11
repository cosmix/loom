//! `loom container list` — list session-backed containers for this workspace.
//!
//! Lists containers tracked in `.work/sessions/`. For orphan containers left
//! behind by a crashed daemon, run `loom clean --sessions`.
//!
//! Each stage runs in its own container, removed on completion / kill / stop / clean.

use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::models::session::{BackendType, Session};
use crate::orchestrator::terminal::container::runtime as rt;
use crate::parser::frontmatter::parse_from_markdown;

/// A single row in the container list output.
pub(crate) struct ListRow {
    pub stage_id: String,
    pub container: String,
    pub runtime: String,
    pub status: String,
    pub session_id: String,
}

/// Render rows as an aligned table.
pub(crate) fn render_table(rows: &[ListRow]) -> String {
    if rows.is_empty() {
        return "No containers found.\n".to_string();
    }

    // Compute column widths (at least as wide as the header).
    let col_stage = rows
        .iter()
        .map(|r| r.stage_id.len())
        .max()
        .unwrap_or(0)
        .max("STAGE".len());
    let col_container = rows
        .iter()
        .map(|r| r.container.len())
        .max()
        .unwrap_or(0)
        .max("CONTAINER".len());
    let col_runtime = rows
        .iter()
        .map(|r| r.runtime.len())
        .max()
        .unwrap_or(0)
        .max("RUNTIME".len());
    let col_status = rows
        .iter()
        .map(|r| r.status.len())
        .max()
        .unwrap_or(0)
        .max("STATUS".len());
    // SESSION_ID is last column — no padding needed.

    let mut out = format!(
        "{:<col_stage$}  {:<col_container$}  {:<col_runtime$}  {:<col_status$}  {}\n",
        "STAGE", "CONTAINER", "RUNTIME", "STATUS", "SESSION_ID"
    );

    for row in rows {
        out.push_str(&format!(
            "{:<col_stage$}  {:<col_container$}  {:<col_runtime$}  {:<col_status$}  {}\n",
            row.stage_id, row.container, row.runtime, row.status, row.session_id
        ));
    }
    out
}

/// Render rows as JSON Lines (one JSON object per line).
pub(crate) fn render_jsonl(rows: &[ListRow]) -> String {
    rows.iter()
        .map(|r| {
            json!({
                "stage":      r.stage_id,
                "container":  r.container,
                "runtime":    r.runtime,
                "status":     r.status,
                "session_id": r.session_id,
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Query the runtime for a container's current state.
///
/// Returns:
/// - `"running"` — container is running
/// - `"missing"` — container does not exist
/// - `"<status>"` — exited or other status string
/// - `"error: <stderr>"` — runtime invocation failure (daemon down, permission denied, …)
fn query_container_status(runtime: rt::Runtime, container_name: &str) -> String {
    let output = match Command::new(runtime.binary())
        .args(["inspect", "-f", "{{.State.Status}}", container_name])
        .output()
    {
        Ok(o) => o,
        Err(e) => return format!("error: {e}"),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if stderr.contains("no such") {
            return "missing".to_string();
        }
        // Daemon-down, permission-denied, etc. — do NOT collapse to missing.
        return format!("error: {}", String::from_utf8_lossy(&output.stderr).trim());
    }

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

pub fn execute(all: bool, json: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let sessions_dir = work_dir.sessions_dir();

    if !sessions_dir.exists() {
        println!(
            "No active loom workspace in {}",
            std::env::current_dir()
                .unwrap_or_else(|_| Path::new(".").to_path_buf())
                .display()
        );
        return Ok(());
    }

    let mut rows = build_rows(&sessions_dir)?;

    // Filter unless --all: only show running.
    if !all {
        rows.retain(|r| r.status == "running");
    }

    // Sort by stage_id ASC.
    rows.sort_by(|a, b| a.stage_id.cmp(&b.stage_id));

    if json {
        let out = render_jsonl(&rows);
        if !out.is_empty() {
            println!("{out}");
        }
    } else {
        print!("{}", render_table(&rows));
    }

    Ok(())
}

/// Build the list of rows by enumerating sessions and querying each runtime.
fn build_rows(sessions_dir: &Path) -> Result<Vec<ListRow>> {
    let mut rows = Vec::new();

    for entry in fs::read_dir(sessions_dir)
        .with_context(|| format!("Failed to read sessions dir {}", sessions_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Warning: failed to read session file {:?}: {e}", path);
                continue;
            }
        };

        let session: Session = match parse_from_markdown(&content, "Session") {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: failed to parse session file {:?}: {e}", path);
                continue;
            }
        };

        if session.backend != BackendType::Container || session.container_name.is_none() {
            continue;
        }

        let container_name = session.container_name.as_deref().unwrap();

        let runtime = session
            .runtime
            .as_deref()
            .and_then(rt::Runtime::from_binary)
            .map(Ok)
            .unwrap_or_else(|| rt::detect_runtime("auto"));

        let (runtime_name, status) = match runtime {
            Ok(rt) => {
                let status = query_container_status(rt, container_name);
                (rt.binary().to_string(), status)
            }
            Err(e) => (
                session.runtime.as_deref().unwrap_or("unknown").to_string(),
                format!("error: {e}"),
            ),
        };

        let stage_id = session.stage_id.as_deref().unwrap_or("unknown").to_string();

        rows.push(ListRow {
            stage_id,
            container: container_name.to_string(),
            runtime: runtime_name,
            status,
            session_id: session.id.clone(),
        });
    }

    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rows() -> Vec<ListRow> {
        vec![
            ListRow {
                stage_id: "stage-b".to_string(),
                container: "loom-stage-b".to_string(),
                runtime: "docker".to_string(),
                status: "running".to_string(),
                session_id: "sess-002".to_string(),
            },
            ListRow {
                stage_id: "stage-a".to_string(),
                container: "loom-stage-a".to_string(),
                runtime: "podman".to_string(),
                status: "exited".to_string(),
                session_id: "sess-001".to_string(),
            },
        ]
    }

    #[test]
    fn render_table_column_order() {
        let rows = make_rows();
        let table = render_table(&rows);
        let header = table.lines().next().unwrap();
        let stage_pos = header.find("STAGE").unwrap();
        let container_pos = header.find("CONTAINER").unwrap();
        let runtime_pos = header.find("RUNTIME").unwrap();
        let status_pos = header.find("STATUS").unwrap();
        let session_pos = header.find("SESSION_ID").unwrap();
        assert!(stage_pos < container_pos, "STAGE before CONTAINER");
        assert!(container_pos < runtime_pos, "CONTAINER before RUNTIME");
        assert!(runtime_pos < status_pos, "RUNTIME before STATUS");
        assert!(status_pos < session_pos, "STATUS before SESSION_ID");
    }

    #[test]
    fn render_table_contains_all_row_data() {
        let rows = make_rows();
        let table = render_table(&rows);
        assert!(table.contains("stage-a"), "missing stage-a");
        assert!(table.contains("stage-b"), "missing stage-b");
        assert!(
            table.contains("loom-stage-a"),
            "missing container loom-stage-a"
        );
        assert!(table.contains("podman"), "missing runtime podman");
        assert!(table.contains("exited"), "missing status exited");
        assert!(table.contains("sess-001"), "missing session_id sess-001");
    }

    #[test]
    fn render_jsonl_valid_json_per_line() {
        let rows = make_rows();
        let jsonl = render_jsonl(&rows);
        for line in jsonl.lines() {
            let val: serde_json::Value =
                serde_json::from_str(line).expect("each line must be valid JSON");
            assert!(val.get("stage").is_some(), "missing 'stage' key");
            assert!(val.get("container").is_some(), "missing 'container' key");
            assert!(val.get("runtime").is_some(), "missing 'runtime' key");
            assert!(val.get("status").is_some(), "missing 'status' key");
            assert!(val.get("session_id").is_some(), "missing 'session_id' key");
        }
    }

    #[test]
    fn render_jsonl_round_trip_values() {
        let rows = make_rows();
        let jsonl = render_jsonl(&rows);
        let values: Vec<serde_json::Value> = jsonl
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();

        // Values appear in original order (not sorted — sorting is done in execute()).
        assert_eq!(values[0]["stage"], "stage-b");
        assert_eq!(values[1]["stage"], "stage-a");
    }

    #[test]
    fn render_table_stable_sort_by_stage_id() {
        // Rows already sorted (as execute() would do before calling render_table).
        let mut rows = make_rows();
        rows.sort_by(|a, b| a.stage_id.cmp(&b.stage_id));
        let table = render_table(&rows);
        let lines: Vec<&str> = table.lines().collect();
        // Header at [0], data at [1..].
        assert!(lines[1].contains("stage-a"), "stage-a should come first");
        assert!(lines[2].contains("stage-b"), "stage-b should come second");
    }

    #[test]
    fn render_table_empty_rows() {
        let table = render_table(&[]);
        assert!(
            table.contains("No containers found"),
            "empty message expected"
        );
    }
}
