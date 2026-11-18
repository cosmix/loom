//! `loom hook codex-backend`: the configured Codex backend, for the shell hooks
//! that refuse the other backend's launch mechanism.

use anyhow::Result;

use crate::codex::{configured_backend, CodexBackend};
use crate::fs::work_dir::WorkDir;

use super::target::non_empty_env;

/// Print `native` or `legacy`: `codex::configured_backend` for the work dir
/// in `LOOM_WORK_DIR` when set, else the current directory's. With no work dir
/// the user tier and the built-in default apply.
pub fn codex_backend() -> Result<()> {
    let hint = non_empty_env("LOOM_WORK_DIR").unwrap_or_else(|| ".".to_string());
    let work_dir = WorkDir::new(hint)?;
    println!("{}", render(configured_backend(work_dir.root())));
    Ok(())
}

fn render(backend: CodexBackend) -> String {
    match backend {
        CodexBackend::Legacy => "legacy",
        CodexBackend::Native => "native",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_each_backend_as_its_config_value() {
        assert_eq!(render(CodexBackend::Legacy), "legacy");
        assert_eq!(render(CodexBackend::Native), "native");
    }
}
