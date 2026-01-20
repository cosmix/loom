//! TUI application for live status dashboard.
//!
//! This module provides the ratatui-based terminal UI for displaying
//! live status updates from the loom daemon.
//!
//! Layout (unified design):
//! - Compact header with spinner, title, and inline progress
//! - Execution graph (scrollable DAG visualization)
//! - Unified stage table with all columns (status, name, merged, deps, elapsed)
//! - Simplified footer with keybinds and errors

mod app;
mod daemon_client;
mod event_handler;
mod renderer;
mod state;

use std::path::Path;

use anyhow::Result;

pub use app::TuiApp;

/// Entry point for TUI live mode.
pub fn run_tui(work_path: &Path) -> Result<()> {
    let mut app = TuiApp::new()?;
    app.run(work_path)
}

