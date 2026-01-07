/// Canonical frontmatter key names used across markdown files.
///
/// Using these constants ensures consistency between file generation and parsing.
/// Always use these constants instead of string literals for frontmatter keys.
pub mod frontmatter {
    // Common identity fields
    pub const ID: &str = "id";
    pub const NAME: &str = "name";

    // Runner-specific fields
    pub const RUNNER_TYPE: &str = "runner_type";
    pub const STATUS: &str = "status";
    pub const ASSIGNED_TRACK: &str = "assigned_track";
    pub const CONTEXT_TOKENS: &str = "context_tokens";
    pub const CONTEXT_LIMIT: &str = "context_limit";

    // Timestamp fields
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const LAST_ACTIVE: &str = "last_active";
    pub const CLOSED_AT: &str = "closed_at";

    // Track-specific fields
    pub const ASSIGNED_RUNNER: &str = "assigned_runner";
    pub const PARENT_TRACK: &str = "parent_track";

    // Signal-specific fields
    pub const TARGET_RUNNER: &str = "target_runner";
    pub const SIGNAL_TYPE: &str = "signal_type";
    pub const PRIORITY: &str = "priority";

    // Handoff-specific fields
    pub const FROM_RUNNER: &str = "from_runner";
    pub const TO_RUNNER: &str = "to_runner";
    pub const TRACK_ID: &str = "track_id";
}

/// Canonical section header names used in markdown documents.
///
/// Using these constants ensures consistent section parsing across the codebase.
pub mod section {
    // Common sections
    pub const METADATA: &str = "Metadata";
    pub const DESCRIPTION: &str = "Description";

    // Runner sections
    pub const IDENTITY: &str = "Identity";
    pub const ASSIGNMENT: &str = "Assignment";
    pub const SESSION_HISTORY: &str = "Session History";

    // Track sections
    pub const CHILD_TRACKS: &str = "Child Tracks";
    pub const CLOSE_REASON: &str = "Close Reason";

    // Signal sections
    pub const TARGET: &str = "Target";
    pub const SIGNAL: &str = "Signal";
    pub const WORK: &str = "Work";
    pub const IMMEDIATE_TASKS: &str = "Immediate Tasks";
    pub const CONTEXT_RESTORATION: &str = "Context Restoration";
    pub const ACCEPTANCE_CRITERIA: &str = "Acceptance Criteria";

    // Handoff sections
    pub const CURRENT_WORK: &str = "Current Work";
    pub const CONTEXT_SUMMARY: &str = "Context Summary";
    pub const PENDING_TASKS: &str = "Pending Tasks";
    pub const COMPLETED_WORK: &str = "Completed Work";
    pub const KEY_DECISIONS: &str = "Key Decisions Made";
    pub const CURRENT_STATE: &str = "Current State";
    pub const NEXT_STEPS: &str = "Next Steps";
    pub const LEARNINGS: &str = "Learnings / Patterns Identified";
}
