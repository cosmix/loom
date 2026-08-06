//! Integration tests for loom orchestration features
//!
//! These tests verify end-to-end behavior of loom's orchestration features
//! including dependency inheritance, worktree management, and conflict resolution.

pub mod dependency_cleanup;
pub mod dependency_conflict;
pub mod dependency_multi;
pub mod dependency_simple;
pub mod helpers;
pub mod hooks_commit_filter;
pub mod hooks_subagent_verify_guard;
pub mod implementer_backwards_compat;
pub mod merge_conflict_recovery;
pub mod plan_verify;
