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
pub(crate) mod ledger;
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
