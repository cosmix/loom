//! A planning session's command catalogue: the native command table, the session's own list
//! read from a headless side process, and the merged table the `/` menu and the send gate use.

mod builtins;
mod list;
mod reader;
mod side;

use std::sync::{Arc, RwLock};

pub use list::{
    CatalogueState, CommandClass, CommandEntry, CommandSource, CommandTable, NAME_CHARS,
};
pub use side::{side_argv, side_settings, OnTable, SideProcess, SideSpec, STARTUP_DEADLINE};

/// A session's current table, replaced whole on each update.
pub type CommandCell = Arc<RwLock<Arc<CommandTable>>>;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_side;
