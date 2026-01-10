//! Signal file operations for worktrees

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Clean up signal files for given session IDs
pub(crate) fn cleanup_signals_for_sessions(
    session_ids: &[String],
    work_dir: &Path,
    verbose: bool,
) -> usize {
    let signals_dir = work_dir.join("signals");
    let mut removed = 0;

    if !signals_dir.exists() {
        return 0;
    }

    for session_id in session_ids {
        let signal_path = signals_dir.join(format!("{session_id}.md"));
        if signal_path.exists() {
            match fs::remove_file(&signal_path) {
                Ok(()) => {
                    removed += 1;
                    if verbose {
                        println!("  Removed signal file: {session_id}.md");
                    }
                }
                Err(e) => {
                    if verbose {
                        eprintln!(
                            "  Warning: Failed to remove signal file '{}': {e}",
                            signal_path.display()
                        );
                    }
                }
            }
        }
    }

    removed
}

/// Remove a single signal file by session ID
pub fn remove_signal_file(session_id: &str, work_dir: &Path) -> Result<bool> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(false);
    }

    fs::remove_file(&signal_path)
        .with_context(|| format!("Failed to remove signal file: {}", signal_path.display()))?;

    Ok(true)
}
