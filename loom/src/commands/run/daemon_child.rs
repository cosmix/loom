//! `loom run --daemon-child <root>`: the daemon process itself.
//!
//! `loom run` starts the daemon by re-executing its own binary with this
//! hidden flag, the absolute state root and the run's config flags
//! (`daemon::server::launch`). The child serves until the daemon stops.

use anyhow::{bail, Result};
use std::path::Path;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::git::signing;

/// Serve as the daemon for the state root `work_root`.
pub(super) fn execute(work_root: &Path, config: DaemonConfig) -> Result<()> {
    if !work_root.is_absolute() {
        bail!(
            "--daemon-child needs an absolute state directory, got '{}'",
            work_root.display()
        );
    }
    // Before any thread exists (`main` spawns none before dispatch, and the
    // server starts its runtime in `serve`): keep the signing variables for
    // the daemon's own signing calls and take them out of the environment
    // every stage process would otherwise inherit. After the path check, so
    // a refused argument (and the test of it) leaves the process untouched.
    signing::take_from_process();
    DaemonServer::with_config(work_root, config).serve()
}

#[cfg(test)]
mod tests {
    use super::super::require_socket_path_fits;
    use super::execute;
    use crate::cli::Cli;
    use crate::daemon::DaemonConfig;
    use crate::fs::work_dir::WorkDir;
    use clap::{CommandFactory, Parser};
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn the_daemon_child_flag_is_hidden_and_refuses_foreground() {
        assert!(Cli::try_parse_from(["loom", "run", "--daemon-child", "/abs"]).is_ok());
        assert!(
            Cli::try_parse_from(["loom", "run", "--daemon-child", "/abs", "--foreground"]).is_err()
        );

        let mut cli = Cli::command();
        let run = cli.find_subcommand_mut("run").expect("run subcommand");
        let help = run.render_help().to_string();
        assert!(!help.contains("daemon-child"), "{help}");
    }

    #[test]
    fn a_relative_work_root_is_refused() {
        let error = execute(Path::new(".loom/work"), DaemonConfig::default())
            .expect_err("a relative state root is refused");

        assert!(error.to_string().contains("absolute"), "{error}");
    }

    #[test]
    fn a_socket_path_past_the_limit_refuses_the_run() {
        let dir = TempDir::new().expect("temp dir");
        let work_dir = WorkDir::new(dir.path().join("r".repeat(100))).expect("work dir");

        let error = require_socket_path_fits(&work_dir).expect_err("a long socket path is refused");

        assert!(error.to_string().contains("104"), "{error}");
    }
}
