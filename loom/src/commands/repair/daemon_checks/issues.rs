//! Diagnostics for daemon observations; all require operator action.

use crate::commands::repair::{DetectedIssue, RepairAction, Severity};

pub(super) fn multiple_daemons(run_pids: &[u32], lock_pid: Option<u32>) -> Option<DetectedIssue> {
    if run_pids.len() <= 1 {
        return None;
    }
    let pid_list = run_pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let keep_hint = match lock_pid {
        Some(pid) => format!("Keep the lock holder (PID {pid}); stop the others with `kill <pid>`"),
        None => "Stop the stale daemons with `kill <pid>` (no lock holder found)".to_string(),
    };
    Some(DetectedIssue {
        action: RepairAction::Manual,
        severity: Severity::Critical,
        description: format!("Multiple 'loom run' processes alive (PIDs: {pid_list})"),
        fix_description: keep_hint,
    })
}

pub(super) fn missing_pid(lock_pid: u32) -> DetectedIssue {
    DetectedIssue {
        action: RepairAction::Manual,
        severity: Severity::Warning,
        description: format!("Daemon lock held (PID {lock_pid}) but orchestrator.pid is missing"),
        fix_description: "Restart the daemon: `loom stop`, then `loom run` (PID file was lost)"
            .to_string(),
    }
}

pub(super) fn missing_socket(lock_pid: u32) -> DetectedIssue {
    DetectedIssue {
        action: RepairAction::Manual,
        severity: Severity::Critical,
        description: format!(
            "Daemon lock held (PID {lock_pid}) but orchestrator.sock is missing (daemon unreachable)"
        ),
        fix_description: "Restart the daemon: `kill <pid>` then `loom run` (control socket was lost)"
            .to_string(),
    }
}

pub(super) fn process_only() -> DetectedIssue {
    DetectedIssue {
        action: RepairAction::Manual,
        severity: Severity::Warning,
        description: "Daemon process appears alive but its socket is unreachable".to_string(),
        fix_description: "Restart the daemon: `loom stop`, then `loom run`".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::repair::fix_issue;

    #[test]
    fn every_daemon_diagnostic_stays_manual_after_wording_changes() {
        let root = tempfile::tempdir().unwrap();
        let mut issues = vec![
            multiple_daemons(&[11, 22], Some(11)).unwrap(),
            missing_pid(11),
            missing_socket(11),
            process_only(),
        ];
        assert!(multiple_daemons(&[11], Some(11)).is_none());
        for issue in &mut issues {
            assert_eq!(issue.action, RepairAction::Manual);
            issue.description = "Changed diagnostic wording".into();
            assert!(!fix_issue(root.path(), issue).unwrap());
        }
    }
}
