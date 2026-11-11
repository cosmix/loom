//! The wire form of a session's command catalogue: the route body and the `commands` frame
//! carry the same fields.

use serde::{Deserialize, Serialize};

use crate::plan_session::commands::CommandEntry;

/// Whether the session's own command list is known; the `/` menu shows the native rows alone
/// while it is `loading` or `unavailable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogueWireState {
    Loading,
    Ready,
    Unavailable,
}

/// A session's command list as the browser reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandsView {
    pub state: CatalogueWireState,
    /// The side process's list number while `ready`.
    pub generation: Option<u64>,
    /// Why the list is `unavailable`.
    pub reason: Option<String>,
    pub entries: Vec<CommandEntry>,
}
