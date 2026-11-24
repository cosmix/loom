//! TUI application for live status dashboard.
//!
//! This module provides the ratatui-based terminal UI for displaying
//! live status updates from the loom daemon.
//!
//! Layout (ledger dashboard):
//! - Compact header with spinner, title, and inline progress
//! - Scheduler alert band, when there are alerts to show
//! - Scrollable stage table with all columns (status, name, merged, deps, elapsed)
//! - Needs-attention and activity panels
//! - One-line footer with the status legend, keybinds, and errors
//! - A legend overlay, toggled with `?`, describing every stage state

mod app;
pub(crate) mod daemon_client;
mod event_handler;
mod handoffs;
pub(crate) mod ledger;
mod plan_overview;
mod plan_overview_format;
mod plan_overview_models;
mod plan_overview_timeline;
mod renderer;
mod state;

#[cfg(test)]
mod plan_overview_contract_tests;
#[cfg(test)]
mod plan_overview_tests;
#[cfg(test)]
mod subscriber_contract_tests;

use std::path::Path;

use anyhow::Result;

pub use app::TuiApp;

/// Entry point for TUI live mode.
pub fn run_tui(work_path: &Path) -> Result<()> {
    let mut app = TuiApp::new()?;
    app.run(work_path)
}

/// Show the finished plan's saved report without a daemon, until the user quits.
/// Returns `false`, without touching the terminal, when the run has no report.
pub fn run_finished_report(work_path: &Path) -> Result<bool> {
    let Some(record) = crate::plan_stats::read_current(work_path) else {
        return Ok(false);
    };
    TuiApp::new()?.run_report(work_path, record)?;
    Ok(true)
}
