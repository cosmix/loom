//! Recent activity log widget

use colored::Colorize;
use std::collections::VecDeque;
use std::time::Instant;

/// Maximum number of activity entries to keep
const MAX_ENTRIES: usize = 10;

/// Type of activity event
#[derive(Debug, Clone)]
pub enum ActivityType {
    StageStarted,
    StageCompleted,
    StageBlocked,
    SessionSpawned,
    SessionCrashed,
    MergeStarted,
    MergeCompleted,
    MergeConflict,
}

/// A single activity entry
#[derive(Debug, Clone)]
pub struct ActivityEntry {
    pub activity_type: ActivityType,
    pub message: String,
    pub timestamp: Instant,
}

impl ActivityEntry {
    pub fn new(activity_type: ActivityType, message: String) -> Self {
        Self {
            activity_type,
            message,
            timestamp: Instant::now(),
        }
    }

    /// Format entry for display
    pub fn format(&self) -> String {
        let elapsed = self.timestamp.elapsed().as_secs();
        let time_str = if elapsed < 60 {
            format!("{elapsed}s ago")
        } else {
            format!("{}m ago", elapsed / 60)
        };

        let (icon, style_fn): (_, fn(_) -> _) = match self.activity_type {
            ActivityType::StageStarted => ("▶", |s: &str| s.blue().to_string()),
            ActivityType::StageCompleted => ("✓", |s: &str| s.green().to_string()),
            ActivityType::StageBlocked => ("✗", |s: &str| s.red().to_string()),
            ActivityType::SessionSpawned => ("○", |s: &str| s.cyan().to_string()),
            ActivityType::SessionCrashed => ("⚠", |s: &str| s.red().to_string()),
            ActivityType::MergeStarted => ("⟳", |s: &str| s.yellow().to_string()),
            ActivityType::MergeCompleted => ("✓", |s: &str| s.green().to_string()),
            ActivityType::MergeConflict => ("⚡", |s: &str| s.red().to_string()),
        };

        format!(
            "{} {} {} {}",
            icon,
            style_fn(&self.message),
            "-".dimmed(),
            time_str.dimmed()
        )
    }
}

/// Activity log maintaining recent events
pub struct ActivityLog {
    entries: VecDeque<ActivityEntry>,
}

impl ActivityLog {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(MAX_ENTRIES),
        }
    }

    /// Add a new activity entry
    pub fn push(&mut self, activity_type: ActivityType, message: impl Into<String>) {
        if self.entries.len() >= MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries
            .push_back(ActivityEntry::new(activity_type, message.into()));
    }

    /// Get recent entries (most recent first)
    pub fn recent(&self, count: usize) -> Vec<&ActivityEntry> {
        self.entries.iter().rev().take(count).collect()
    }

    /// Render activity log
    pub fn render(&self, count: usize) -> Vec<String> {
        self.recent(count).into_iter().map(|e| e.format()).collect()
    }

    /// Check if log is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new()
    }
}
