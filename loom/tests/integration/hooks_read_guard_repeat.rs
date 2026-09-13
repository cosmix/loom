//! Repeat-read and tier-one knowledge-hint cases for
//! `hooks_read_guard.rs`. The parent owns the shared hook harness.

use super::*;

#[test]
fn binary_extension_skips_rule_one_even_when_large() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "big.png", 500);
    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let output = run_read_hook(
        &hook,
        json!({"file_path": file.to_string_lossy()}),
        &session,
        Some(&covered_stub_dir(stubs.path())),
    );

    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty(), "stdout={}", output.stdout);
}

#[test]
fn non_read_tool_and_missing_file_path_are_silently_ignored() {
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new();
    let wrong_tool = json!({
        "tool_name": "Bash", "tool_input": {"command": "echo hi"}, "agent_id": session.agent_id,
    });
    let output = run_payload(&hook, &wrong_tool, &session, None);
    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty() && output.stderr.trim().is_empty());

    let output = run_read_hook(&hook, json!({}), &session, None);
    assert_eq!(output.code, 0, "stderr={}", output.stderr);
    assert!(output.stdout.trim().is_empty() && output.stderr.trim().is_empty());
}

#[test]
fn reads_ledger_row_is_tab_separated_path_kind_lines_timestamp() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "small.rs", 50);
    let session = Session::new();
    let stub_dir = receipt_stub_dir(stubs.path());
    let output = run_read_hook(
        &hook,
        json!({"file_path": file.to_string_lossy(), "offset": 5, "limit": 10}),
        &session,
        Some(&stub_dir),
    );
    assert_eq!(output.code, 0, "stderr={}", output.stderr);

    let ledger = session.work_dir().join(format!(
        "hooks/reads/{}/{}.tsv",
        session.session_id, session.agent_id
    ));
    let content = fs::read_to_string(&ledger).expect("read ledger");
    let row: Vec<&str> = content.trim_end().split('\t').collect();
    assert_eq!(row.len(), 4, "row={row:?}");
    assert_eq!(row[0], file.to_string_lossy());
    assert_eq!(row[1], "range");
    assert_eq!(row[2], "5-15");
    assert!(!row[3].is_empty());
}

#[test]
fn tier1_hint_is_unscoped_for_unset_invalid_and_real_stage_ids() {
    for stage_id in [None, Some("not a valid stage"), Some("context-admission")] {
        let (_hook_dir, hook) = setup_hook();
        let files = TempDir::new().expect("files dir");
        let stubs = TempDir::new().expect("stubs dir");
        let path = files.path().join("doc/loom/knowledge/topic.md");
        let parent = path.parent().expect("knowledge parent");
        fs::create_dir_all(parent).expect("create knowledge dir");
        fs::write(&path, "line\n").expect("write knowledge file");
        let session = Session::new().with_stage_id(stage_id);
        let stub_dir = receipt_stub_dir(stubs.path());
        let output = run_read_hook(
            &hook,
            json!({"file_path": path.to_string_lossy()}),
            &session,
            Some(&stub_dir),
        );

        assert_eq!(
            output.code, 0,
            "stage={stage_id:?} stderr={}",
            output.stderr
        );
        let hint = warn_context(&output.stdout);
        assert!(
            hint.contains("loom knowledge context --query"),
            "hint={hint}"
        );
        assert!(!hint.contains("--stage"), "hint={hint}");
    }
}

fn assert_knowledge_read(
    hook: &Path,
    session: &Session,
    stub_dir: &Path,
    path: &Path,
    expect_exempt: bool,
) {
    let out = run_read_hook(
        hook,
        json!({"file_path": path.to_string_lossy()}),
        session,
        Some(stub_dir),
    );
    if expect_exempt {
        assert_eq!(out.code, 0, "stderr={}", out.stderr);
        assert!(
            warn_context(&out.stdout).contains("loom knowledge context"),
            "stdout={}",
            out.stdout
        );
    } else {
        assert_eq!(
            out.code, 2,
            "tier-2 file must not be exempt: stdout={} stderr={}",
            out.stdout, out.stderr
        );
    }
}

fn assert_knowledge_read_silent(hook: &Path, session: &Session, stub_dir: &Path, path: &Path) {
    let out = run_read_hook(
        hook,
        json!({"file_path": path.to_string_lossy()}),
        session,
        Some(stub_dir),
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

#[test]
fn tier1_knowledge_read_warns_index_is_exempt_tier2_is_not_exempt() {
    let test = "repeat::tier1_knowledge_read_warns_index_is_exempt_tier2_is_not_exempt";
    if skip_unless_gate_visible(test) {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let knowledge_dir = files.path().join("doc/loom/knowledge");
    fs::create_dir_all(&knowledge_dir).expect("create knowledge dir");

    let tier1 = knowledge_dir.join("mistakes.md");
    fs::write(&tier1, "line\n".repeat(500)).expect("write tier-1 file");
    assert_knowledge_read(&hook, &session, &stub_dir, &tier1, true);

    let index = knowledge_dir.join("INDEX.md");
    fs::write(&index, "line\n".repeat(500)).expect("write index");
    assert_knowledge_read_silent(&hook, &session, &stub_dir, &index);

    let tier2_dir = knowledge_dir.join("mistakes");
    fs::create_dir_all(&tier2_dir).expect("create tier-2 dir");
    let tier2 = tier2_dir.join("refactor-stragglers.md");
    fs::write(&tier2, "line\n".repeat(500)).expect("write tier-2 file");
    assert_knowledge_read(&hook, &session, &stub_dir, &tier2, false);
}
