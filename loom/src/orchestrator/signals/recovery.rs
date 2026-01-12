//! Recovery signal generation for crashed/hung sessions.
//!
//! When a session crashes or hangs, the orchestrator generates a recovery signal
//! that contains context about what was happening and how to continue.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;

use super::generate::build_embedded_context_with_stage;
use super::types::EmbeddedContext;

/// Type of recovery being initiated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RecoveryReason {
    /// Session crashed (PID dead)
    Crash,
    /// Session hung (PID alive, no heartbeat)
    Hung,
    /// Context exhaustion (PreCompact fired)
    ContextExhaustion,
    /// Manual recovery triggered by user
    Manual,
}

impl std::fmt::Display for RecoveryReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryReason::Crash => write!(f, "Session crashed"),
            RecoveryReason::Hung => write!(f, "Session hung"),
            RecoveryReason::ContextExhaustion => write!(f, "Context exhaustion"),
            RecoveryReason::Manual => write!(f, "Manual recovery"),
        }
    }
}

/// Content for a recovery signal
#[derive(Debug, Clone)]
pub struct RecoverySignalContent {
    /// New session ID for the recovery session
    pub session_id: String,
    /// Stage being recovered
    pub stage_id: String,
    /// Previous session ID that crashed/hung
    pub previous_session_id: String,
    /// Reason for recovery
    pub reason: RecoveryReason,
    /// Time when the issue was detected
    pub detected_at: DateTime<Utc>,
    /// Last heartbeat information (if available)
    pub last_heartbeat: Option<LastHeartbeatInfo>,
    /// Crash report path (if available)
    pub crash_report_path: Option<PathBuf>,
    /// Suggested recovery actions
    pub recovery_actions: Vec<String>,
    /// How many times this stage has been recovered
    pub recovery_attempt: u32,
}

/// Information from the last heartbeat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastHeartbeatInfo {
    /// When the heartbeat was recorded
    pub timestamp: DateTime<Utc>,
    /// Context percentage at the time
    pub context_percent: Option<f32>,
    /// Last tool being used
    pub last_tool: Option<String>,
    /// Activity description
    pub activity: Option<String>,
}

impl RecoverySignalContent {
    /// Create a new recovery signal for a crashed session
    pub fn for_crash(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        crash_report_path: Option<PathBuf>,
        recovery_attempt: u32,
    ) -> Self {
        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::Crash,
            detected_at: Utc::now(),
            last_heartbeat: None,
            crash_report_path,
            recovery_actions: vec![
                "Review the crash report for error details".to_string(),
                "Continue work from the last known state".to_string(),
                "If the issue persists, check for environmental problems".to_string(),
            ],
            recovery_attempt,
        }
    }

    /// Create a new recovery signal for a hung session
    pub fn for_hung(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        last_heartbeat: Option<LastHeartbeatInfo>,
        recovery_attempt: u32,
    ) -> Self {
        let mut recovery_actions = vec![
            "Review the last known activity before hang".to_string(),
            "Check if the operation was waiting for external resources".to_string(),
        ];

        if let Some(ref hb) = last_heartbeat {
            if let Some(ref tool) = hb.last_tool {
                recovery_actions.insert(0, format!("Previous session was using: {}", tool));
            }
        }

        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::Hung,
            detected_at: Utc::now(),
            last_heartbeat,
            crash_report_path: None,
            recovery_actions,
            recovery_attempt,
        }
    }

    /// Create a new recovery signal for context exhaustion
    pub fn for_context_exhaustion(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        context_percent: f32,
        recovery_attempt: u32,
    ) -> Self {
        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::ContextExhaustion,
            detected_at: Utc::now(),
            last_heartbeat: Some(LastHeartbeatInfo {
                timestamp: Utc::now(),
                context_percent: Some(context_percent),
                last_tool: None,
                activity: Some("Context limit reached".to_string()),
            }),
            crash_report_path: None,
            recovery_actions: vec![
                "Read the handoff file carefully for context".to_string(),
                "Continue from the documented progress".to_string(),
                "Prioritize completing remaining tasks efficiently".to_string(),
            ],
            recovery_attempt,
        }
    }

    /// Create a new recovery signal for manual recovery
    pub fn for_manual(
        session_id: String,
        stage_id: String,
        previous_session_id: String,
        recovery_attempt: u32,
    ) -> Self {
        Self {
            session_id,
            stage_id,
            previous_session_id,
            reason: RecoveryReason::Manual,
            detected_at: Utc::now(),
            last_heartbeat: None,
            crash_report_path: None,
            recovery_actions: vec![
                "Review any available handoff or crash reports".to_string(),
                "Check the current state of the stage's work".to_string(),
                "Continue from where the previous session left off".to_string(),
            ],
            recovery_attempt,
        }
    }

    /// Set custom recovery actions
    pub fn with_recovery_actions(mut self, actions: Vec<String>) -> Self {
        self.recovery_actions = actions;
        self
    }

    /// Add a recovery action
    pub fn add_recovery_action(&mut self, action: String) {
        self.recovery_actions.push(action);
    }
}

/// Generate a recovery signal file
pub fn generate_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");
    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    // Build embedded context including any available handoff
    let handoff_file = find_latest_handoff_for_stage(work_dir, &content.stage_id);
    let embedded_context = build_embedded_context_with_stage(
        work_dir,
        handoff_file.as_deref(),
        Some(&content.stage_id),
    );

    let signal_path = signals_dir.join(format!("{}.md", content.session_id));
    let signal_content = format_recovery_signal(content, stage, &embedded_context);

    fs::write(&signal_path, signal_content)
        .with_context(|| format!("Failed to write recovery signal: {}", signal_path.display()))?;

    Ok(signal_path)
}

/// Find the latest handoff file for a stage
fn find_latest_handoff_for_stage(work_dir: &Path, stage_id: &str) -> Option<String> {
    let handoffs_dir = work_dir.join("handoffs");
    if !handoffs_dir.exists() {
        return None;
    }

    let mut handoffs: Vec<_> = fs::read_dir(&handoffs_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.ends_with(".md") && name_str.contains(stage_id)
        })
        .collect();

    // Sort by modification time, newest first
    handoffs.sort_by_key(|e| {
        std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok()))
    });

    handoffs.first().map(|e| {
        e.path()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    })
}

/// Format a recovery signal as markdown
fn format_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    embedded_context: &EmbeddedContext,
) -> String {
    let mut signal = String::new();

    // Header
    signal.push_str(&format!("# Recovery Signal: {}\n\n", content.session_id));

    // Recovery context
    signal.push_str("## Recovery Context\n\n");
    signal.push_str("**This is a RECOVERY session.** The previous session encountered an issue.\n\n");
    signal.push_str(&format!("- **Reason**: {}\n", content.reason));
    signal.push_str(&format!("- **Previous Session**: {}\n", content.previous_session_id));
    signal.push_str(&format!("- **Recovery Attempt**: #{}\n", content.recovery_attempt));
    signal.push_str(&format!("- **Detected At**: {}\n", content.detected_at.format("%Y-%m-%d %H:%M:%S UTC")));

    if let Some(ref crash_path) = content.crash_report_path {
        signal.push_str(&format!("- **Crash Report**: {}\n", crash_path.display()));
    }

    signal.push_str("\n");

    // Last heartbeat info
    if let Some(ref hb) = content.last_heartbeat {
        signal.push_str("### Last Known State\n\n");
        signal.push_str(&format!("- **Timestamp**: {}\n", hb.timestamp.format("%Y-%m-%d %H:%M:%S UTC")));
        if let Some(pct) = hb.context_percent {
            signal.push_str(&format!("- **Context Usage**: {:.1}%\n", pct));
        }
        if let Some(ref tool) = hb.last_tool {
            signal.push_str(&format!("- **Last Tool**: {}\n", tool));
        }
        if let Some(ref activity) = hb.activity {
            signal.push_str(&format!("- **Activity**: {}\n", activity));
        }
        signal.push_str("\n");
    }

    // Recovery actions
    signal.push_str("### Recovery Actions\n\n");
    for (i, action) in content.recovery_actions.iter().enumerate() {
        signal.push_str(&format!("{}. {}\n", i + 1, action));
    }
    signal.push_str("\n");

    // Worktree context
    signal.push_str("## Worktree Context\n\n");
    signal.push_str("You are in an **isolated git worktree**. This signal contains everything you need:\n\n");
    signal.push_str("- **Your stage assignment and acceptance criteria are below** - this file is self-contained\n");
    signal.push_str("- **All context (plan overview, handoff, knowledge) is embedded below** - reading main repo files is **FORBIDDEN**\n");
    signal.push_str("- **Commit to your worktree branch** - it will be merged after verification\n\n");

    // Target information
    signal.push_str("## Target\n\n");
    signal.push_str(&format!("- **Session**: {}\n", content.session_id));
    signal.push_str(&format!("- **Stage**: {}\n", content.stage_id));
    if let Some(ref plan_id) = stage.plan_id {
        signal.push_str(&format!("- **Plan**: {}\n", plan_id));
    }
    if let Some(ref worktree) = stage.worktree {
        signal.push_str(&format!("- **Worktree**: {}\n", worktree));
    }
    signal.push_str(&format!("- **Branch**: loom/{}\n", content.stage_id));
    signal.push_str("\n");

    // Assignment from stage
    signal.push_str("## Assignment\n\n");
    signal.push_str(&format!("{}\n\n", stage.name));
    if let Some(ref desc) = stage.description {
        signal.push_str(&format!("{}\n\n", desc));
    }

    // Acceptance criteria
    if !stage.acceptance.is_empty() {
        signal.push_str("## Acceptance Criteria\n\n");
        for criteria in &stage.acceptance {
            signal.push_str(&format!("- [ ] {}\n", criteria));
        }
        signal.push_str("\n");
    }

    // Files to modify
    if !stage.files.is_empty() {
        signal.push_str("## Files to Modify\n\n");
        for file in &stage.files {
            signal.push_str(&format!("- {}\n", file));
        }
        signal.push_str("\n");
    }

    // Embedded context - handoff
    if let Some(ref handoff) = embedded_context.handoff_content {
        signal.push_str("## Previous Session Handoff\n\n");
        signal.push_str("<handoff>\n");
        signal.push_str(handoff);
        signal.push_str("\n</handoff>\n\n");
    }

    // Embedded context - plan overview
    if let Some(ref overview) = embedded_context.plan_overview {
        signal.push_str("## Plan Overview\n\n");
        signal.push_str("<plan-overview>\n");
        signal.push_str(overview);
        signal.push_str("\n</plan-overview>\n\n");
    }

    signal
}

/// Read a recovery signal file
pub fn read_recovery_signal(work_dir: &Path, session_id: &str) -> Result<Option<RecoverySignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{}.md", session_id));
    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path)
        .with_context(|| format!("Failed to read recovery signal: {}", signal_path.display()))?;

    // Check if this is a recovery signal by looking for the recovery context section
    if !content.contains("## Recovery Context") {
        return Ok(None);
    }

    // Parse basic information from the signal
    // Note: This is a simplified parser that extracts key fields
    let stage_id = extract_field(&content, "Stage:")
        .unwrap_or_default()
        .to_string();
    let previous_session_id = extract_field(&content, "Previous Session:")
        .unwrap_or_default()
        .to_string();
    let recovery_attempt = extract_field(&content, "Recovery Attempt:")
        .and_then(|s| s.trim_start_matches('#').parse().ok())
        .unwrap_or(1);

    let reason = if content.contains("Session crashed") {
        RecoveryReason::Crash
    } else if content.contains("Session hung") {
        RecoveryReason::Hung
    } else if content.contains("Context exhaustion") {
        RecoveryReason::ContextExhaustion
    } else {
        RecoveryReason::Manual
    };

    Ok(Some(RecoverySignalContent {
        session_id: session_id.to_string(),
        stage_id,
        previous_session_id,
        reason,
        detected_at: Utc::now(), // We don't parse this from the file
        last_heartbeat: None,
        crash_report_path: None,
        recovery_actions: vec![],
        recovery_attempt,
    }))
}

/// Extract a field value from markdown content
fn extract_field<'a>(content: &'a str, field: &str) -> Option<&'a str> {
    for line in content.lines() {
        if line.contains(field) {
            if let Some(value) = line.split(field).nth(1) {
                let value = value.trim().trim_start_matches("**").trim_end_matches("**");
                return Some(value);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_stage() -> Stage {
        Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: Some("Test description".to_string()),
            status: crate::models::stage::StageStatus::Executing,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec!["cargo test".to_string()],
            setup: vec![],
            files: vec!["src/lib.rs".to_string()],
            plan_id: Some("test-plan".to_string()),
            worktree: Some(".worktrees/test-stage".to_string()),
            session: Some("session-123".to_string()),
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
            auto_merge: None,
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
        }
    }

    #[test]
    fn test_recovery_signal_for_crash() {
        let content = RecoverySignalContent::for_crash(
            "session-new".to_string(),
            "test-stage".to_string(),
            "session-old".to_string(),
            Some(PathBuf::from(".work/crashes/crash-123.md")),
            1,
        );

        assert_eq!(content.reason, RecoveryReason::Crash);
        assert_eq!(content.session_id, "session-new");
        assert_eq!(content.previous_session_id, "session-old");
        assert_eq!(content.recovery_attempt, 1);
        assert!(content.crash_report_path.is_some());
    }

    #[test]
    fn test_recovery_signal_for_hung() {
        let hb = LastHeartbeatInfo {
            timestamp: Utc::now(),
            context_percent: Some(45.0),
            last_tool: Some("Bash".to_string()),
            activity: Some("Running tests".to_string()),
        };

        let content = RecoverySignalContent::for_hung(
            "session-new".to_string(),
            "test-stage".to_string(),
            "session-old".to_string(),
            Some(hb),
            2,
        );

        assert_eq!(content.reason, RecoveryReason::Hung);
        assert!(content.last_heartbeat.is_some());
        assert_eq!(content.recovery_attempt, 2);
    }

    #[test]
    fn test_generate_recovery_signal() -> Result<()> {
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();

        // Create signals directory
        fs::create_dir_all(work_dir.join("signals"))?;

        let stage = create_test_stage();
        let content = RecoverySignalContent::for_crash(
            "session-recovery".to_string(),
            "test-stage".to_string(),
            "session-crashed".to_string(),
            None,
            1,
        );

        let path = generate_recovery_signal(&content, &stage, work_dir)?;
        assert!(path.exists());

        let signal_content = fs::read_to_string(&path)?;
        assert!(signal_content.contains("## Recovery Context"));
        assert!(signal_content.contains("Session crashed"));
        assert!(signal_content.contains("session-crashed"));

        Ok(())
    }
}
