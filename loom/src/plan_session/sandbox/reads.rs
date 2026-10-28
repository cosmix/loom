//! The read denies of a planning session: `sandbox.filesystem.denyRead` and the runtime grants
//! carved out of it (`allowRead`). The guard recomputes them through [`read_denies`] so layer 1
//! and layer 3 deny the same paths.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::filesystem::text;
use crate::fs::permissions::state_root::{resolve_state_root, token_deny_paths};
use crate::planning::runtime::DRAFTS_DIR;
use crate::sandbox::settings::policy::mandatory_deny_read;

/// The state root's terminal history directory. `orchestrator/terminal/tmux/history.rs:58`
/// (`HISTORY_DIR`) names it privately, so the name is repeated here once.
const TERMINAL_HISTORY_DIR: &str = "terminal-history";

/// The runtime grants under the runtime directory, as `RuntimePaths` names them
/// (`planning/runtime.rs`, `RuntimePaths::new`): `drafts`, `claims`, `summaries.lock`.
const RUNTIME_GRANTS: [&str; 3] = [DRAFTS_DIR, "claims", "summaries.lock"];

/// The strings a planning session's settings emit for reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadDenies {
    /// `denyRead`: `~/`-prefixed credential entries, then absolute paths (the state-root token
    /// files in the `//abs` form `token_deny_paths` produces).
    pub denied: Vec<String>,
    /// `allowRead`: the runtime grants every kind keeps readable.
    pub carve_outs: Vec<String>,
}

/// The read denies of a session over `root` whose runtime directory is `runtime_dir`.
///
/// Denied: the credential paths of `policy::mandatory_deny_read(false)` (no planning session
/// runs codex), the planning runtime root (the parent of `runtime_dir`) canonicalized so a
/// `LOOM_HOME` redirect is followed, `<home>/.loom/plan` when it differs, and, when the
/// project's state root resolves, its token files and terminal history.
///
/// A path that is not valid UTF-8 is refused: the planning root, the home root and the state
/// root are resolved through symlinks, so a valid input can still reach a non-UTF-8 name, and a
/// lossy conversion would deny a path that does not exist instead of the real one.
pub fn read_denies(home: &Path, root: &Path, runtime_dir: &Path) -> Result<ReadDenies> {
    let mut denied: Vec<String> = mandatory_deny_read(false)
        .into_iter()
        .map(str::to_owned)
        .collect();
    let planning_root = canonical_or_lexical(runtime_dir.parent().unwrap_or(runtime_dir));
    let home_root = canonical_or_lexical(&home.join(".loom").join("plan"));
    denied.push(text(&planning_root)?);
    if home_root != planning_root {
        denied.push(text(&home_root)?);
    }
    if let Some(state_root) = resolve_state_root(root) {
        denied.extend(token_deny_paths(&text(&state_root)?));
        denied.push(text(&state_root.join(TERMINAL_HISTORY_DIR))?);
    }
    let carve_outs = RUNTIME_GRANTS
        .iter()
        .map(|name| text(&runtime_dir.join(name)))
        .collect::<Result<_>>()?;
    Ok(ReadDenies { denied, carve_outs })
}

/// `path` canonicalized when it exists, else as given: a runtime root not created yet is still
/// denied by the path it will have.
fn canonical_or_lexical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
