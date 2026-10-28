//! `loom plan hook`: the hook command Claude Code runs for planning sessions.

use std::path::Path;

/// Runs the hook command Claude Code invokes for planning sessions.
pub fn execute(event: &str, session_dir: &Path) -> anyhow::Result<()> {
    let code = crate::plan_session::hook_cli::run(
        event,
        session_dir,
        std::io::stdin().lock(),
        std::io::stdout().lock(),
    );
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}
