use super::*;

#[test]
fn unbounded_read_of_large_covered_file_denies_with_outline() {
    if skip_unless_gate_visible("cases::unbounded_read_of_large_covered_file_denies_with_outline") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);

    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let tool_input = json!({"file_path": file.to_string_lossy()});
    let out = run_read_hook(&hook, tool_input, &session, Some(&stub_dir));

    assert_eq!(out.code, 2, "stdout={} stderr={}", out.stdout, out.stderr);
    assert!(out.stderr.contains("500"), "stderr={}", out.stderr);
    assert!(
        out.stderr.contains("POLL_INTERVAL") && out.stderr.contains("gather"),
        "stderr={}",
        out.stderr
    );
    assert!(
        out.stderr
            .contains("Read the ranges you need with offset/limit"),
        "stderr={}",
        out.stderr
    );
    assert!(
        !out.stderr.contains(STUB_STDERR_WARNING),
        "stub stderr leaked: {}",
        out.stderr
    );
}

#[test]
fn bounded_read_of_large_file_is_allowed() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.rs", 500);

    let session = Session::new();
    session.enable_deny();
    let tool_input = json!({"file_path": file.to_string_lossy(), "offset": 0, "limit": 100});
    let out = run_read_hook(&hook, tool_input, &session, Some(&stub_dir));

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

#[test]
fn line_limit_boundary_is_strictly_greater_than() {
    if skip_unless_gate_visible("cases::line_limit_boundary_is_strictly_greater_than") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = covered_stub_dir(stubs.path());
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    let at_limit = write_file_with_lines(files.path(), "exactly400.rs", 400);
    let out = run_read_hook(
        &hook,
        json!({"file_path": at_limit.to_string_lossy()}),
        &session,
        Some(&stub_dir),
    );
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);

    let over_limit = write_file_with_lines(files.path(), "over401.rs", 401);
    let out = run_read_hook(
        &hook,
        json!({"file_path": over_limit.to_string_lossy()}),
        &session,
        Some(&stub_dir),
    );
    assert_eq!(out.code, 2, "stderr={}", out.stderr);
}

#[test]
fn uncovered_file_warns_and_allows_even_with_deny_on() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let stub_dir = uncovered_stub_dir(stubs.path());
    let file = write_file_with_lines(files.path(), "big.sh", 500);

    let session = Session::new();
    session.enable_deny();
    let out = run_read_hook(
        &hook,
        json!({"file_path": file.to_string_lossy()}),
        &session,
        Some(&stub_dir),
    );

    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stderr.is_empty(), "stderr={}", out.stderr);
    let ctx = warn_context(&out.stdout);
    assert!(
        ctx.contains("rg -n") && ctx.contains("offset/limit"),
        "ctx={ctx}"
    );
}

#[test]
fn repeated_binary_reads_stay_silent() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let stubs = TempDir::new().expect("stubs dir");
    let file = write_file_with_lines(files.path(), "icon.png", 50);
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    for n in 1..=3 {
        let out = run_read_hook(
            &hook,
            json!({"file_path": file.to_string_lossy()}),
            &session,
            Some(&lexical_stub_dir(stubs.path())),
        );
        assert_eq!(out.code, 0, "read {n}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "read {n}: stdout={}",
            out.stdout
        );
    }
}

#[test]
fn pdf_pages_bounded_reads_are_never_denied() {
    let (_hook_dir, hook) = setup_hook();
    let files = TempDir::new().expect("files dir");
    let file = write_file_with_lines(files.path(), "spec.pdf", 50);
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    for pages in ["1-20", "21-40", "41-60"] {
        let out = run_read_hook(
            &hook,
            json!({"file_path": file.to_string_lossy(), "pages": pages}),
            &session,
            None,
        );
        assert_eq!(out.code, 0, "pages={pages}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "pages={pages}: stdout={}",
            out.stdout
        );
    }
}
