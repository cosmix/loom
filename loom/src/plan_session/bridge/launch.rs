//! What a session is launched with and the request that starts one.

use std::path::PathBuf;

use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::Origin;

/// What a session is launched with (plan 8 supplies it through the guidance seam).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchGuidance {
    pub brief_path: PathBuf,
    pub plugin_dir: PathBuf,
    /// Pasted after the first `SessionStart`, never on the launch line (px-security-04).
    pub opening_message: String,
    pub model: String,
    pub effort: String,
    pub display_name: String,
    /// Pass the opening as the launch line's prompt instead of pasting it; accepted only for
    /// a command and a plan path (px-seams-R04).
    pub opening_on_argv: bool,
}
/// Writes the session's guidance files into its directory and describes the launch.
pub type GuidanceFn = Box<dyn FnOnce(&SessionDir) -> anyhow::Result<LaunchGuidance> + Send>;
/// A request to start one session.
pub struct StartRequest {
    pub kind: SessionKind,
    pub guidance: GuidanceFn,
    /// The origin of the opening's user item.
    pub opening_origin: Option<Origin>,
    /// The `Offer::id` the chat started from.
    pub offer: Option<String>,
    /// The feature the session belongs to; `None` for a standalone or project-wide session.
    pub feature: Option<String>,
    /// The standalone plan's slug the session belongs to; `None` for any other session.
    pub plan: Option<String>,
    pub extra_env: Vec<(String, String)>,
}
