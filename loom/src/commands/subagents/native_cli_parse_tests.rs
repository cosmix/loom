use clap::Parser;

use super::*;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: SubagentsCommand,
}

fn parses(args: &[&str]) -> bool {
    Cli::try_parse_from(std::iter::once("loom").chain(args.iter().copied())).is_ok()
}
#[test]
fn provider_must_be_codex() {
    for (provider, valid) in [("codex", true), ("other", false)] {
        assert_eq!(
            parses(&[
                "start",
                "--provider",
                provider,
                "--unit-id",
                "unit",
                "--brief",
                "/brief",
                "--model",
                "gpt-6.2-sol",
                "--effort",
                "high"
            ]),
            valid
        );
    }
}
fn parses_message(tail: &[&str]) -> bool {
    let mut args = vec![
        "message",
        "--worker",
        "codex:unit",
        "--invocation",
        INV,
        "--message-id",
        "msg",
    ];
    args.extend_from_slice(tail);
    parses(&args)
}
#[test]
fn message_needs_exactly_one_of_text_and_file() {
    for (tail, valid) in [
        (vec![], false),
        (vec!["--text", "hi"], true),
        (vec!["--file", "/file"], true),
        (vec!["--text", "hi", "--file", "/file"], false),
    ] {
        let mut args = vec!["--delivery", "steer"];
        args.extend(tail);
        assert_eq!(parses_message(&args), valid);
    }
}
#[test]
fn cancel_stage_and_invocation_conflict() {
    let base = ["cancel", "--worker", "codex:unit", "--reason", "stop"];
    for (tail, valid) in [
        (vec![], false),
        (vec!["--stage", "stage-a"], true),
        (vec!["--invocation", INV], true),
        (vec!["--stage", "stage-a", "--invocation", INV], false),
    ] {
        let mut args = base.to_vec();
        args.extend(tail);
        assert_eq!(parses(&args), valid);
    }
}
#[test]
fn delivery_takes_steer_or_next_turn_only() {
    for (delivery, valid) in [
        ("steer", true),
        ("next-turn", true),
        ("next_turn", false),
        ("other", false),
    ] {
        assert_eq!(
            parses_message(&["--delivery", delivery, "--text", "hi"]),
            valid
        );
    }
}
