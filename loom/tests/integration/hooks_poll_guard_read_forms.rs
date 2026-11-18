//! Which Bash-side reads of a large file rule 1 counts as whole reads, for
//! `hooks_poll_guard.rs`: output sent to a pipe or a file never reaches the agent, a relative
//! argument after a `cd` names some other file, and the control-input exemption and the rule's
//! message hold for the forms an agent actually types. Shares the parent's harness via
//! `use super::*`.

use super::reads::{assert_denied_once, assert_silently_allowed};
use super::*;

fn run_in(hook: &Path, command: &str, session: &Session, stubs: &Path, cwd: &Path) -> HookOutput {
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": {"command": command},
        "agent_id": session.agent_id,
        "session_id": session.session_id,
    });
    run_payload_in(hook, &payload, session, Some(stubs), Some(cwd))
}

#[test]
fn output_sent_to_a_pipe_or_file_spends_no_denial() {
    if skip_unless_gate_visible("read_forms::output_sent_to_a_pipe_or_file_spends_no_denial") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.json", 500);
    let path = file.to_string_lossy();

    let sinks = [
        "| wc -l",
        "| jq .x",
        "|& wc -l",
        "> copy",
        ">> copy",
        ">| copy",
        "1> copy",
        "&> copy",
        "2>/dev/null | wc -l",
        "2>&1 | wc -l",
        "< /dev/null | wc -l",
    ];
    for sink in sinks {
        let session = Session::new().with_live_main_agent();
        let piped = run_bash_hook(
            &hook,
            &format!("cat {path} {sink}"),
            &session,
            Some(&stub_dir),
        );
        assert_silently_allowed(&piped, sink);
        let plain = run_bash_hook(&hook, &format!("cat {path}"), &session, Some(&stub_dir));
        assert_denied_once(&plain, sink);
        let repeat = run_bash_hook(&hook, &format!("cat {path}"), &session, Some(&stub_dir));
        assert_silently_allowed(&repeat, sink);
    }

    let session = Session::new().with_live_main_agent();
    let sed = format!("sed -n '1,$p' {path} | wc -l");
    let out = run_bash_hook(&hook, &sed, &session, Some(&stub_dir));
    assert_silently_allowed(&out, &sed);
    let after_splice = format!("bash -c 'echo a'; cat {path} | wc -l");
    let out = run_bash_hook(&hook, &after_splice, &session, Some(&stub_dir));
    assert_silently_allowed(&out, &after_splice);
    let inner = format!("bash -c 'cat {path} | wc -l'");
    let out = run_bash_hook(&hook, &inner, &session, Some(&stub_dir));
    assert_silently_allowed(&out, &inner);
}

#[test]
fn output_that_still_reaches_the_agent_is_denied() {
    if skip_unless_gate_visible("read_forms::output_that_still_reaches_the_agent_is_denied") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.json", 500);
    let path = file.to_string_lossy();

    let commands = [
        format!("cat {path} 2>/dev/null"),
        format!("cat {path} 2>&1"),
        format!("cat {path} >&2"),
        format!("cat {path}; echo x"),
        format!("cat {path} && echo x"),
        format!("cat {path} || true"),
        format!("cat {path} &"),
        format!("cat {path}\necho x | wc -l"),
        format!("cat {path} '|'"),
        format!("cat {path} \"a | b\""),
        format!("cat {path} 'x > y'"),
        format!("echo a | bash -c 'echo b | wc -l'; cat {path}"),
    ];
    for command in commands {
        let session = Session::new().with_live_main_agent();
        let out = run_bash_hook(&hook, &command, &session, Some(&stub_dir));
        assert_denied_once(&out, &command);
    }
}

#[test]
fn a_relative_argument_after_a_directory_change_names_some_other_file() {
    if skip_unless_gate_visible(
        "read_forms::a_relative_argument_after_a_directory_change_names_some_other_file",
    ) {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let root = TempDir::new().expect("root dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    write_file_with_lines(root.path(), "README.md", 500);
    fs::create_dir(root.path().join("sub")).expect("create sub");

    for moved in [
        "cd sub && cat README.md",
        "cd sub; cat README.md",
        "pushd sub && cat README.md",
    ] {
        let session = Session::new().with_live_main_agent();
        let out = run_in(&hook, moved, &session, &stub_dir, root.path());
        assert_silently_allowed(&out, moved);
        let plain = run_in(&hook, "cat README.md", &session, &stub_dir, root.path());
        assert_denied_once(&plain, moved);
    }

    let session = Session::new().with_live_main_agent();
    let order = "cat README.md && cd sub";
    let out = run_in(&hook, order, &session, &stub_dir, root.path());
    assert_denied_once(&out, order);
}

#[test]
fn an_absolute_argument_after_a_directory_change_is_still_checked() {
    if skip_unless_gate_visible(
        "read_forms::an_absolute_argument_after_a_directory_change_is_still_checked",
    ) {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);

    let command = format!("cd /tmp && cat {}", file.display());
    let session = Session::new().with_live_main_agent();
    let out = run_bash_hook(&hook, &command, &session, Some(&stub_dir));
    assert_denied_once(&out, &command);
}

#[test]
fn a_relative_plan_read_from_inside_doc_plans_is_exempt() {
    let (_hook_dir, hook) = setup_hook();
    let root = TempDir::new().expect("root dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let plans = root.path().join("doc/plans");
    fs::create_dir_all(&plans).expect("create doc/plans");
    write_file_with_lines(&plans, "PLAN-x.md", 500);

    let session = Session::new().with_live_main_agent();
    let out = run_in(&hook, "cat PLAN-x.md", &session, &stub_dir, &plans);
    assert_silently_allowed(&out, "cat PLAN-x.md");
}

#[test]
fn a_capped_markdown_outline_keeps_its_remainder_line() {
    if skip_unless_gate_visible("read_forms::a_capped_markdown_outline_keeps_its_remainder_line") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let body = "-> Outline: stub\n  L1-L2\theading\t\t# One\n... 3 more headings\ncoverage: markdown headings";
    write_exec(
        &stubs.path().join("loom"),
        &format!("#!/usr/bin/env bash\ncat <<'EOF'\n{body}\nEOF\n"),
    );
    let file = write_file_with_lines(files.path(), "big.md", 500);

    let session = Session::new().with_live_main_agent();
    let command = format!("cat {}", file.display());
    let out = run_bash_hook(&hook, &command, &session, Some(stubs.path()));
    assert_denied_once(&out, &command);
    assert!(
        out.stderr.contains("... 3 more headings"),
        "stderr={}",
        out.stderr
    );
    assert!(!out.stderr.contains("coverage:"), "stderr={}", out.stderr);
}

#[test]
fn the_search_advice_quotes_a_path_with_a_space() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    write_exec(&stubs.path().join("loom"), "#!/usr/bin/env bash\nexit 0\n");
    let file = write_file_with_lines(files.path(), "my big.txt", 500);

    let session = Session::new();
    let command = format!("cat '{}'", file.display());
    let out = run_bash_hook(&hook, &command, &session, Some(stubs.path()));
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    let advice = warn_context(&out.stdout);
    assert!(advice.contains("my\\ big.txt`"), "{advice}");
}
