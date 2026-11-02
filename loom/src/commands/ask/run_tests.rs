use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, Instant};

use clap::{CommandFactory, Parser};

use super::{is_scrubbed, run_worker, Finished, Worker, SESSION_VARS, STDOUT_CAP};
use crate::commands::ask::backend::{printable, Backend};
use crate::commands::ask::AskArgs;

const CLAUDE_CAPTURE: &str = include_str!("../../../tests/fixtures/ask/ask-claude.json");
const CODEX_CAPTURE: &str = include_str!("../../../tests/fixtures/ask/ask-codex.jsonl");

fn shell(script: &str, payload: &str, timeout: Duration) -> anyhow::Result<Finished> {
    let args = ["-c".to_owned(), script.to_owned()];
    run_worker(&Worker {
        program: Path::new("/bin/sh"),
        args: &args,
        env: &[("ASK_TEST_MARK", "1")],
        payload,
        timeout,
    })
}

fn failure(script: &str, timeout: Duration) -> String {
    let error = shell(script, "", timeout).expect_err("the worker failed");
    format!("{error:#}")
}

#[test]
fn the_payload_arrives_on_stdin() {
    let finished = shell("cat", "first\nlast\n", Duration::from_secs(30)).expect("cat");
    assert_eq!(finished.stdout, "first\nlast\n");
}

#[test]
fn the_environment_is_scrubbed_and_extended() {
    let finished = shell("env", "", Duration::from_secs(30)).expect("env");
    let lines: Vec<&str> = finished.stdout.lines().collect();
    assert!(lines.contains(&"ASK_TEST_MARK=1"), "{lines:?}");
    assert!(
        !lines.iter().any(|line| line.starts_with("LOOM_")),
        "{lines:?}"
    );
}

#[test]
fn a_nonzero_exit_fails_with_a_stderr_excerpt() {
    let message = failure("echo boom >&2; exit 3", Duration::from_secs(30));
    assert!(message.contains("boom"), "{message}");
    assert!(message.contains('3'), "{message}");
}

#[test]
fn stdout_past_sixteen_mib_fails() {
    let at_cap = format!("head -c {STDOUT_CAP} /dev/zero");
    let finished = shell(&at_cap, "", Duration::from_secs(60)).expect("output at the cap");
    assert_eq!(finished.stdout.len(), STDOUT_CAP);
    let over = format!("head -c {} /dev/zero", STDOUT_CAP + 1);
    let message = failure(&over, Duration::from_secs(60));
    assert!(message.contains("cap"), "{message}");
}

#[test]
fn the_deadline_kills_the_worker() {
    let started = Instant::now();
    let message = failure("sleep 30", Duration::from_secs(1));
    assert!(message.contains("timed out"), "{message}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn control_characters_are_stripped_from_the_answer() {
    assert_eq!(printable("a\u{1b}[2Jb\r\n\tc\u{7}"), "a[2Jb\n\tc");
    let hostile =
        CLAUDE_CAPTURE.replace("\"result\":\"ok\"", "\"result\":\"o\\u001b]0;x\\u0007k\"");
    assert_ne!(hostile, CLAUDE_CAPTURE, "the capture lost its result field");
    let reply = Backend::Claude.parse(&hostile).expect("parse");
    assert_eq!(reply.answer, "o]0;xk");
}

#[test]
fn bidi_controls_are_spaced_out_of_the_answer() {
    let hostile = "a\u{202e}b\u{2066}c\u{2069}d\u{202a}e\nf\tg";
    assert_eq!(printable(hostile), "a b c d e\nf\tg");
}

#[test]
fn a_nonzero_exit_without_stderr_shows_the_stdout_error() {
    let script = r#"echo '{"is_error":true,"result":"quota hit"}'; exit 1"#;
    let message = failure(script, Duration::from_secs(30));
    assert!(message.contains("quota hit"), "{message}");
}

#[test]
fn stderr_wins_over_stdout_in_a_failure() {
    let message = failure("echo out; echo err >&2; exit 1", Duration::from_secs(30));
    assert!(
        message.contains("err") && !message.contains("out"),
        "{message}"
    );
}

#[test]
fn an_absurd_timeout_is_a_usage_error() {
    let parse =
        |value: &str| AskArgs::try_parse_from(["ask", "--eval", "c.json", "--timeout-secs", value]);
    assert!(parse(&u64::MAX.to_string()).is_err());
    assert!(parse("86401").is_err());
    assert!(parse("0").is_err());
    assert_eq!(parse("86400").expect("parse").timeout_secs, 86400);
}

#[test]
fn claude_capture_parses_and_an_error_result_fails() {
    let reply = Backend::Claude.parse(CLAUDE_CAPTURE).expect("parse");
    assert_eq!(reply.answer, "ok");
    let counts = (
        reply.input_tokens,
        reply.cache_read_tokens,
        reply.cache_write_tokens,
        reply.output_tokens,
    );
    assert_eq!(counts, (2, 0, 608, 4));
    let errored =
        CLAUDE_CAPTURE.replace("\"subtype\":\"success\"", "\"subtype\":\"error_max_turns\"");
    assert!(Backend::Claude.parse(&errored).is_err());
}

#[test]
fn codex_capture_subtracts_cached_input() {
    let reply = Backend::Codex.parse(CODEX_CAPTURE).expect("parse");
    assert_eq!(reply.answer, "ok");
    let counts = (
        reply.input_tokens,
        reply.cache_read_tokens,
        reply.cache_write_tokens,
        reply.output_tokens,
    );
    assert_eq!(counts, (8680, 11520, 0, 5));
}

#[test]
fn codex_fails_closed() {
    let tool = r#"{"type":"item.completed","item":{"id":"x","type":"file_change"}}"#;
    let failed = r#"{"type":"turn.failed","error":{"message":"quota"}}"#;
    for extra in [tool, failed] {
        let output = format!("{extra}\n{CODEX_CAPTURE}");
        assert!(
            Backend::Codex.parse(&output).is_err(),
            "{extra} was accepted"
        );
    }
    let unfinished: String = CODEX_CAPTURE
        .lines()
        .filter(|line| !line.contains("turn.completed"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert!(Backend::Codex.parse(&unfinished).is_err());
}

#[test]
fn question_and_eval_exclude_each_other() {
    let parse = |args: &[&str]| AskArgs::try_parse_from(args.iter().copied());
    assert!(parse(&["ask", "--question", "q", "--paths", "a", "--eval", "c.json"]).is_err());
    assert!(parse(&["ask"]).is_err());
    assert!(parse(&["ask", "--question", "q"]).is_err());
    assert!(parse(&["ask", "--eval", "c.json", "--json"]).is_err());
    assert!(parse(&["ask", "--eval", "c.json", "--paths", "a"]).is_err());
    let args = parse(&["ask", "--question", "q", "--paths", "a", "b"]).expect("parse");
    assert_eq!(args.paths.len(), 2);
    assert_eq!(args.backend, Backend::Claude);
    assert_eq!(args.timeout_secs, 180);
    assert!(args.model.is_none());
    assert!(parse(&["ask", "--eval", "c.json", "--backend", "codex"]).is_ok());
}

#[test]
fn every_session_variable_and_the_loom_prefix_are_scrubbed() {
    assert_eq!(SESSION_VARS.len(), 7);
    for name in SESSION_VARS.iter().copied().chain(["LOOM_STAGE_ID"]) {
        assert!(is_scrubbed(OsStr::new(name)), "{name} reaches the worker");
    }
    for name in ["PATH", "HOME", "CLAUDE_CODE_DISABLE_FILE_MENTIONS", "LOOM"] {
        assert!(!is_scrubbed(OsStr::new(name)), "{name} was scrubbed");
    }
}

#[test]
fn codex_fails_closed_on_a_tool_item_in_any_phase() {
    for phase in ["item.started", "item.updated"] {
        for item in [r#"{"id":"x","type":"command_execution"}"#, r#"{"id":"x"}"#] {
            let row = format!(r#"{{"type":"{phase}","item":{item}}}"#);
            let output = format!("{row}\n{CODEX_CAPTURE}");
            assert!(Backend::Codex.parse(&output).is_err(), "{row} was accepted");
        }
    }
}

#[test]
fn codex_message_and_reasoning_may_stream_through_every_phase() {
    let stream = [
        r#"{"type":"item.started","item":{"id":"r","type":"reasoning"}}"#,
        r#"{"type":"item.updated","item":{"id":"m","type":"agent_message"}}"#,
        r#"{"type":"item.completed","item":{"id":"r","type":"reasoning","text":"hm"}}"#,
    ]
    .join("\n");
    let reply = Backend::Codex
        .parse(&format!("{stream}\n{CODEX_CAPTURE}"))
        .expect("parse");
    assert_eq!(reply.answer, "ok");
}

#[test]
fn help_marks_the_codex_lane_experimental() {
    let help = AskArgs::command().render_long_help().to_string();
    let lane = help
        .lines()
        .find(|line| line.trim_start().starts_with("- codex"))
        .unwrap_or_else(|| panic!("no codex lane in:\n{help}"));
    assert!(lane.contains("Experimental"), "{help}");
}
