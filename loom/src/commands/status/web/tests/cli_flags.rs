//! `loom status --web` and `--terminals` argument parsing.

use clap::CommandFactory;

use crate::cli::Cli;
use crate::commands::status::web::DEFAULT_PORT;

#[test]
fn the_cli_preserves_omitted_and_explicit_dashboard_ports() {
    let absent = Cli::command()
        .try_get_matches_from(["loom", "status"])
        .expect("parse `loom status`");
    let absent_status = absent
        .subcommand_matches("status")
        .expect("status subcommand");
    assert!(!absent_status.contains_id("web"));

    let bare = Cli::command()
        .try_get_matches_from(["loom", "status", "--web"])
        .expect("parse `loom status --web`");
    let bare_status = bare
        .subcommand_matches("status")
        .expect("status subcommand");
    assert!(bare_status.contains_id("web"));
    assert_eq!(bare_status.get_one::<u16>("web"), None);

    for port in [0, DEFAULT_PORT] {
        let port = port.to_string();
        let expected = port.parse::<u16>().expect("valid test port");
        let matches = Cli::command()
            .try_get_matches_from(["loom", "status", "--web", &port])
            .expect("parse explicit dashboard port");
        let status = matches
            .subcommand_matches("status")
            .expect("status subcommand");
        assert!(status.contains_id("web"));
        assert_eq!(status.get_one::<u16>("web"), Some(&expected));
    }
}

#[test]
fn terminals_requires_web() {
    let error = Cli::command()
        .try_get_matches_from(["loom", "status", "--terminals"])
        .expect_err("terminals alone must fail");
    assert!(error.to_string().contains("required"), "{error}");
}

#[test]
fn terminals_parses_with_web() {
    let matches = Cli::command()
        .try_get_matches_from(["loom", "status", "--web", "--terminals"])
        .expect("parse terminals dashboard");
    let status = matches
        .subcommand_matches("status")
        .expect("status command");
    assert!(status.get_flag("terminals"));
}
