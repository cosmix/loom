use anyhow::Result;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;

/// Context for generating a diagnosis signal
pub struct DiagnosisContext {
    pub stage: Stage,
    pub crash_report: Option<String>,
    pub log_tail: Option<String>,
    pub git_status: Option<String>,
    pub git_diff: Option<String>,
}

/// Generate a diagnosis signal file for a session
pub fn generate_diagnosis_signal(
    ctx: &DiagnosisContext,
    session_id: &str,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    let failure_type = ctx
        .stage
        .failure_info
        .as_ref()
        .map(|i| format!("{:?}", i.failure_type))
        .unwrap_or_else(|| "Unknown".to_string());

    let content = format!(
        r#"# Diagnosis Signal: {session_id}

## Context
You are diagnosing a failed stage. Analyze the evidence and provide actionable recommendations.

## Target
- **Session**: {session_id}
- **Stage**: {stage_id}
- **Stage Name**: {stage_name}

## Stage Information
- **Status**: Blocked
- **Failure Type**: {failure_type}
- **Close Reason**: {close_reason}
- **Retry Count**: {retry_count}

## Crash Report
<crash-report>
{crash_report}
</crash-report>

## Recent Log Output
<log-tail>
{log_tail}
</log-tail>

## Git Status (Worktree)
<git-status>
{git_status}
</git-status>

## Recent Git Diff
<git-diff>
{git_diff}
</git-diff>

## Your Task

1. **Analyze** the failure evidence above
2. **Identify** the root cause
3. **Write a diagnosis report** to `.work/diagnoses/{stage_id}.md`
4. **Recommend** one of these actions:
   - `retry` - If transient failure, retry will likely succeed
   - `fix` - Provide specific fix instructions in the diagnosis
   - `skip` - If stage can be skipped safely
   - `escalate` - Needs human intervention

## Diagnosis Report Format

Write your diagnosis to `.work/diagnoses/{stage_id}.md`:

```markdown
---
stage_id: {stage_id}
failure_type: {failure_type}
root_cause: "Brief description"
recommended_action: retry|fix|skip|escalate
diagnosed_at: "{timestamp}"
---

# Diagnosis: {stage_name}

## Root Cause
[Detailed explanation]

## Evidence
- [Evidence 1]
- [Evidence 2]

## Recommended Fix
[Fix instructions if applicable]

## Risk Assessment
[What could go wrong]
```
"#,
        session_id = session_id,
        stage_id = ctx.stage.id,
        stage_name = ctx.stage.name,
        failure_type = failure_type,
        close_reason = ctx
            .stage
            .close_reason
            .as_deref()
            .unwrap_or("No reason provided"),
        retry_count = ctx.stage.retry_count,
        crash_report = ctx
            .crash_report
            .as_deref()
            .unwrap_or("No crash report available"),
        log_tail = ctx.log_tail.as_deref().unwrap_or("No log output available"),
        git_status = ctx
            .git_status
            .as_deref()
            .unwrap_or("Git status not available"),
        git_diff = ctx.git_diff.as_deref().unwrap_or("No uncommitted changes"),
        timestamp = Utc::now().to_rfc3339(),
    );

    // Ensure signals directory exists
    if let Some(parent) = signal_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&signal_path, content)?;
    Ok(signal_path)
}

/// Load crash report content for a stage
pub fn load_crash_report(stage_id: &str, work_dir: &Path) -> Option<String> {
    let crashes_dir = work_dir.join("crashes");
    if !crashes_dir.exists() {
        return None;
    }

    // Find the most recent crash report for this stage
    let entries = fs::read_dir(&crashes_dir).ok()?;
    let mut crash_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(stage_id))
        .collect();

    crash_files.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));

    crash_files
        .last()
        .and_then(|entry| fs::read_to_string(entry.path()).ok())
}
