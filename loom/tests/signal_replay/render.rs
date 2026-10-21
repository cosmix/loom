//! Renders every stage signal and every worker query of the frozen plan.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{bail, Context, Result};
use loom::models::session::Session;
use loom::models::stage::{Stage, StageStatus, StageType};
use loom::models::worktree::Worktree;
use loom::orchestrator::signals::{
    generate_signal_with_metrics, generate_signal_with_skills, DependencyStatus,
};
use loom::verify::transitions::update_stage;
use serde_json::{json, Value};

use super::{helpers, input};

/// One worker spawn to replay.
pub struct Query {
    pub stage_id: String,
    pub id: String,
    pub subagent_type: String,
    pub description: String,
    pub prompt: String,
}

/// What one worker query returned.
pub struct Issued {
    pub query_bytes: usize,
    pub brief_bytes: usize,
    pub envelope_bytes: usize,
}

fn kebab(stage_type: StageType) -> String {
    serde_json::to_value(stage_type)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn set_status(work_dir: &Path, id: &str, status: StageStatus, merged: bool) -> Result<()> {
    update_stage(id, work_dir, |s| {
        s.status = status.clone();
        if merged {
            s.merged = true;
        }
        Ok(())
    })
    .with_context(|| format!("update stage {id}"))?;
    Ok(())
}

/// Signal report lines, in stage order. `requests` maps stage id to its count.
pub fn signal_lines(work_dir: &Path, stages: &[Stage], requests: &Value) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    for stage in stages {
        if stage.stage_type == StageType::Knowledge {
            lines.push(format!(
                "signal\t{}\tknowledge\tskipped=knowledge-signal",
                stage.id
            ));
            set_status(work_dir, &stage.id, StageStatus::Completed, true)?;
            continue;
        }
        set_status(work_dir, &stage.id, StageStatus::Executing, false)?;
        lines.push(render_stage(work_dir, stage, requests)?);
        set_status(work_dir, &stage.id, StageStatus::Completed, true)?;
    }
    Ok(lines)
}

fn render_stage(work_dir: &Path, stage: &Stage, requests: &Value) -> Result<String> {
    let mut session = Session::new();
    session.id = format!("session-replay-{}", stage.id);
    // A fixed path: the dynamic section prints it, so it must not follow the temp root.
    let worktree = Worktree::new(
        stage.id.clone(),
        PathBuf::from("/replay/.worktrees").join(&stage.id),
        format!("loom/{}", stage.id),
    );
    let deps: Vec<DependencyStatus> = stage
        .dependencies
        .iter()
        .map(|d| DependencyStatus {
            stage_id: d.clone(),
            name: d.clone(),
            status: "Completed".to_string(),
            outputs: vec![],
        })
        .collect();
    let (_, m) =
        generate_signal_with_metrics(&session, stage, &worktree, &deps, None, None, work_dir)?;
    let path = generate_signal_with_skills(
        &session,
        stage,
        &worktree,
        &deps,
        None,
        None,
        work_dir,
        None,
        &[],
    )?;
    let file = fs::metadata(&path)?.len();
    let n = request_count(requests, &["stages", &stage.id])?;
    Ok(format!(
        "signal\t{}\t{}\tstable_prefix={}\tsemi_stable={}\tdynamic={}\trecitation={}\ttotal={}\tfile={}\trequests={}\tresident_estimate={}",
        stage.id, kebab(stage.stage_type), m.stable_prefix_bytes, m.semi_stable_bytes,
        m.dynamic_bytes, m.recitation_bytes, m.signal_size_bytes, file, n,
        m.signal_size_bytes as u64 * n
    ))
}

pub fn request_count(requests: &Value, path: &[&str]) -> Result<u64> {
    let mut v = requests;
    for key in path {
        v = &v[*key];
    }
    v.as_u64()
        .with_context(|| format!("requests.json lacks {}", path.join(".")))
}

/// The recorded spawn followed by one query per non-codex worker row.
pub fn queries(stages: &[Stage]) -> Result<Vec<Query>> {
    let raw = fs::read_to_string(input::fixture_dir().join("recorded-spawn.json"))?;
    let recorded: Value = serde_json::from_str(&raw)?;
    let tool_input = &recorded["tool_input"];
    let field = |k: &str| tool_input[k].as_str().map(str::to_string);
    let mut out = vec![Query {
        stage_id: "token-core".to_string(),
        id: "recorded-1".to_string(),
        subagent_type: field("subagent_type").context("recorded subagent_type")?,
        description: field("description").context("recorded description")?,
        prompt: field("prompt").context("recorded prompt")?,
    }];
    for stage in stages {
        if matches!(
            stage.stage_type,
            StageType::Standard | StageType::IntegrationVerify
        ) {
            out.extend(table_queries(stage));
        }
    }
    Ok(out)
}

fn table_queries(stage: &Stage) -> Vec<Query> {
    let mut out: Vec<Query> = Vec::new();
    for row in stage
        .description
        .as_deref()
        .unwrap_or("")
        .lines()
        .map(str::trim)
    {
        let inner = row.trim_matches('|');
        let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
        let is_worker = row.starts_with('|')
            && cells.len() >= 6
            && cells[0].len() > 1
            && cells[0].starts_with(['W', 'C'])
            && cells[0][1..].bytes().all(|b| b.is_ascii_digit());
        if !is_worker || cells[2].contains("codex") {
            continue;
        }
        let subagent_type = if cells[2].contains("opus") {
            "loom-senior-software-engineer"
        } else {
            "loom-software-engineer"
        };
        out.push(Query {
            stage_id: stage.id.clone(),
            id: format!("w{}", out.len() + 1),
            subagent_type: subagent_type.to_string(),
            description: format!("{} {}", cells[0], cells[1]),
            prompt: format!(
                "Your brief: {}. Read it in full before anything else.\n\n{row}",
                cells[5]
            ),
        });
    }
    out
}

/// Run `loom hook worker-brief` for one query.
pub fn issue(root: &Path, home: &Path, q: &Query) -> Result<Issued> {
    let payload = json!({
        "tool_name": "Agent",
        "tool_input": {
            "subagent_type": q.subagent_type, "prompt": q.prompt, "description": q.description
        },
        "session_id": "claude-replay",
        "cwd": root,
        "transcript_path": root.join("replay.jsonl"),
    });
    let mut child = helpers::loom_cmd()
        .args(["hook", "worker-brief"])
        .current_dir(root)
        .env("LOOM_STAGE_ID", &q.stage_id)
        .env("LOOM_WORK_DIR", root.join(".loom/work"))
        .env("LOOM_SESSION_ID", format!("session-replay-{}", q.stage_id))
        .env("LOOM_HOOK_CONTEXT", "1")
        .env("HOME", home)
        .env("LOOM_HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn loom hook worker-brief")?;
    child
        .stdin
        .take()
        .context("child stdin")?
        .write_all(payload.to_string().as_bytes())?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "worker-brief {} failed: {}",
            q.id,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let line = String::from_utf8(output.stdout)?;
    let envelope = line.trim_end_matches('\n');
    let value: Value = serde_json::from_str(envelope).context("worker-brief stdout is JSON")?;
    Ok(Issued {
        query_bytes: q.description.len() + 1 + q.prompt.len(),
        brief_bytes: value["brief"].as_str().map_or(0, str::len),
        envelope_bytes: envelope.len(),
    })
}
