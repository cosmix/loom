//! Receipt-lifecycle cases for `hooks_read_guard.rs`.

use super::*;

fn write_read_transcript(
    session: &Session,
    tool_input: &Value,
    tool_use_id: &str,
    result: &str,
    is_error: bool,
) -> PathBuf {
    let transcript = session
        .work_dir()
        .join(format!("read-transcript-{tool_use_id}.jsonl"));
    let assistant = json!({
        "type": "assistant",
        "message": {"content": [{
            "type": "tool_use", "id": tool_use_id, "name": "Read", "input": tool_input,
        }]},
    });
    let user = json!({
        "type": "user",
        "message": {"content": [{
            "type": "tool_result", "tool_use_id": tool_use_id, "content": result,
            "is_error": is_error,
        }]},
    });
    fs::write(&transcript, format!("{assistant}\n{user}\n")).expect("write transcript");
    transcript
}

fn complete_read_receipt(
    session: &Session,
    tool_input: &Value,
    tool_use_id: &str,
    result: &str,
    is_error: bool,
) {
    let transcript = write_read_transcript(session, tool_input, tool_use_id, result, is_error);
    let payload = json!({
        "tool_name": "Read", "tool_input": tool_input, "tool_use_id": tool_use_id,
        "transcript_path": transcript, "agent_id": session.agent_id,
        "session_id": session.session_id, "cwd": session.work_dir(),
    });
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    command.args(["hook", "read-receipt", "--complete"]);
    configure_command_environment(&mut command, session, None);
    command
        .current_dir(session.work_dir())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().expect("spawn receipt completion");
    child
        .stdin
        .take()
        .expect("receipt stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("write receipt payload");
    let output = child
        .wait_with_output()
        .expect("wait for receipt completion");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
}

fn prepare_read(hook: &Path, session: &Session, stub_dir: &Path, input: &Value) {
    let output = run_read_hook(hook, input.clone(), session, Some(stub_dir));
    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty(), "stdout={}", output.stdout);
}

fn assert_receipt_warns(output: &HookOutput) {
    assert!(
        warn_context(&output.stdout).contains("read in full at"),
        "stdout={}",
        output.stdout
    );
}

#[test]
fn attempted_read_without_completion_does_not_escalate() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let input = json!({"file_path": file.to_string_lossy()});
    let session = Session::new();
    let stub_dir = receipt_stub_dir(stubs.path());

    prepare_read(&hook, &session, &stub_dir, &input);
    let output = run_read_hook(&hook, input, &session, Some(&stub_dir));

    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty(), "stdout={}", output.stdout);
}

#[test]
fn completed_matching_text_read_escalates_the_repeat() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let input = json!({"file_path": file.to_string_lossy()});
    let session = Session::new();
    let stub_dir = receipt_stub_dir(stubs.path());

    prepare_read(&hook, &session, &stub_dir, &input);
    complete_read_receipt(&session, &input, "read-1", "line\n", false);
    let output = run_read_hook(&hook, input, &session, Some(&stub_dir));

    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(
        warn_context(&output.stdout).contains("read in full at"),
        "stdout={}",
        output.stdout
    );
}

#[test]
fn two_proven_full_reads_deny_the_third_when_enabled() {
    let test = "receipts::two_proven_full_reads_deny_the_third_when_enabled";
    if skip_unless_gate_visible(test) {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let input = json!({"file_path": file.to_string_lossy()});
    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let stub_dir = receipt_stub_dir(stubs.path());

    prepare_read(&hook, &session, &stub_dir, &input);
    complete_read_receipt(&session, &input, "read-1", "line\n", false);
    let second = run_read_hook(&hook, input.clone(), &session, Some(&stub_dir));
    assert_eq!(second.code, 0, "stderr={}", second.stderr);
    assert!(warn_context(&second.stdout).contains("read in full at"));
    complete_read_receipt(&session, &input, "read-2", "line\n", false);
    let third = run_read_hook(&hook, input, &session, Some(&stub_dir));

    assert_eq!(
        third.code, 2,
        "stdout={} stderr={}",
        third.stdout, third.stderr
    );
    assert!(
        third
            .stderr
            .contains("has been read in full 2 times already"),
        "stderr={}",
        third.stderr
    );
}

#[test]
fn two_proven_identical_ranges_warn_on_the_third_read() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let input = json!({"file_path": file.to_string_lossy(), "offset": 5, "limit": 10});
    let session = Session::new();
    let stub_dir = receipt_stub_dir(stubs.path());

    prepare_read(&hook, &session, &stub_dir, &input);
    complete_read_receipt(&session, &input, "read-1", "line\n", false);
    prepare_read(&hook, &session, &stub_dir, &input);
    complete_read_receipt(&session, &input, "read-2", "line\n", false);
    let third = run_read_hook(&hook, input, &session, Some(&stub_dir));

    assert_eq!(third.code, 0, "stderr={}", third.stderr);
    assert!(
        warn_context(&third.stdout).contains("range 5-15")
            && warn_context(&third.stdout).contains("has been read 2 times"),
        "stdout={}",
        third.stdout
    );
}

#[test]
fn source_edit_after_completion_resets_repeat_eligibility() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let input = json!({"file_path": file.to_string_lossy()});
    let session = Session::new();
    let stub_dir = receipt_stub_dir(stubs.path());
    prepare_read(&hook, &session, &stub_dir, &input);
    complete_read_receipt(&session, &input, "read-1", "line\n", false);
    assert_receipt_warns(&run_read_hook(
        &hook,
        input.clone(),
        &session,
        Some(&stub_dir),
    ));
    fs::write(&file, "changed\n").expect("edit source");
    let output = run_read_hook(&hook, input, &session, Some(&stub_dir));
    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty(), "stdout={}", output.stdout);
}

#[test]
fn error_result_does_not_escalate_a_repeat() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let input = json!({"file_path": file.to_string_lossy()});
    let control_file = write_file_with_lines(files.path(), "control.rs", 50);
    let control_input = json!({"file_path": control_file.to_string_lossy()});
    let session = Session::new();
    let stub_dir = receipt_stub_dir(stubs.path());
    prepare_read(&hook, &session, &stub_dir, &control_input);
    complete_read_receipt(&session, &control_input, "control-read", "line\n", false);
    assert_receipt_warns(&run_read_hook(
        &hook,
        control_input.clone(),
        &session,
        Some(&stub_dir),
    ));
    prepare_read(&hook, &session, &stub_dir, &input);
    complete_read_receipt(&session, &input, "read-1", "Read failed", true);
    let output = run_read_hook(&hook, input, &session, Some(&stub_dir));
    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty(), "stdout={}", output.stdout);
}
