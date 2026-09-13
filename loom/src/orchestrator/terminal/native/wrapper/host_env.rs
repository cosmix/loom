//! The host facts a session wrapper exports so loom's hooks run only
//! executables sessions cannot write: `LOOM_SCRATCH_DIR` (where the CLI stages
//! relay tickets), `LOOM_BIN` (the daemon's own verified binary) and
//! `LOOM_HOOK_PATH` (the daemon's PATH minus every session-writable root).
//! `native::launch` resolves them; this module only renders them.

use shell_escape::escape;
use std::path::PathBuf;

use super::CONTINUATION;

/// The wrapper's host exports; the default renders nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WrapperHostEnv {
    /// `<scratch_root>/<session-id>`, exported as `LOOM_SCRATCH_DIR`.
    pub scratch_dir: Option<PathBuf>,
    /// The verified loom binary, exported as `LOOM_BIN`.
    pub loom_bin: Option<PathBuf>,
    /// The filtered PATH entries, exported colon-joined as `LOOM_HOOK_PATH`.
    pub hook_path: Vec<PathBuf>,
}

impl WrapperHostEnv {
    /// Shell-escaped `exec env` assignments, one continuation line each, in
    /// the shape of the wrapper's other exports.
    pub(super) fn render(&self) -> String {
        let hook_path = (!self.hook_path.is_empty()).then(|| {
            self.hook_path
                .iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(":")
        });
        [
            (
                "LOOM_SCRATCH_DIR",
                self.scratch_dir
                    .as_ref()
                    .map(|dir| dir.display().to_string()),
            ),
            (
                "LOOM_BIN",
                self.loom_bin.as_ref().map(|bin| bin.display().to_string()),
            ),
            ("LOOM_HOOK_PATH", hook_path),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            let assignment = escape(format!("{name}={}", value?).into());
            Some(format!("    {assignment} {CONTINUATION}"))
        })
        .collect()
    }
}
