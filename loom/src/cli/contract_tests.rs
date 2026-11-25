//! Contract test: `init` and `run` have no `--backend` option, while
//! `loom ask --backend` keeps its own.

use clap::error::ErrorKind;
use clap::Parser;

use super::Cli;

#[test]
fn retired_backend_flag_is_rejected_on_init_and_run() {
    for argv in [
        &["loom", "init", "p.md", "--backend", "tmux"][..],
        &["loom", "run", "--backend", "native"][..],
        &["loom", "run", "--foreground", "--backend", "tmux"][..],
    ] {
        match Cli::try_parse_from(argv) {
            Ok(_) => panic!("{argv:?} must be rejected"),
            Err(err) => assert_eq!(err.kind(), ErrorKind::UnknownArgument, "{argv:?}: {err}"),
        }
    }
    for argv in [
        &["loom", "init", "p.md"][..],
        &["loom", "run"][..],
        &[
            "loom",
            "ask",
            "--question",
            "q",
            "--paths",
            "a.rs",
            "--backend",
            "codex",
        ][..],
    ] {
        if let Err(err) = Cli::try_parse_from(argv) {
            panic!("{argv:?} must parse: {err}");
        }
    }
}
