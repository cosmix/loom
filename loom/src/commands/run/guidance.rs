//! What `loom run` tells the operator to do next.

use colored::Colorize;
use std::path::Path;

/// Print how to follow a running daemon: the two status views and the absolute
/// path of its log. The daemon runs detached, so everything it reports,
/// failures and stalls included, ends up in `orchestrator.log` rather than in
/// this terminal.
pub(super) fn write_follow_guidance(
    out: &mut impl std::io::Write,
    work_dir: &Path,
) -> std::io::Result<()> {
    let log = work_dir.join("orchestrator.log");
    writeln!(
        out,
        "  {}  Monitor progress in the terminal",
        "loom status --live".cyan()
    )?;
    writeln!(
        out,
        "  {}  Monitor progress in the browser",
        "loom status --web".cyan()
    )?;
    writeln!(
        out,
        "  {}  Daemon log: failures and stalls",
        log.display().to_string().cyan()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn follow_guidance_names_status_live_status_web_and_the_log() {
        let dir = TempDir::new().unwrap();
        let mut out = Vec::new();
        write_follow_guidance(&mut out, dir.path()).unwrap();

        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("loom status --live"), "{text}");
        assert!(text.contains("loom status --web"), "{text}");
        let log = dir.path().join("orchestrator.log");
        assert!(text.contains(&log.display().to_string()), "{text}");
    }
}
