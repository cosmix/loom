//! Persisted output: only a file Claude Code itself could have written, under
//! `~/.claude/projects/**/tool-results/`, is read.

use super::*;
use serde_json::json;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use tempfile::TempDir;

/// `<tmp>/home/.claude/projects` holding one session's `tool-results` dir.
struct Projects {
    tmp: TempDir,
    root: PathBuf,
    results: PathBuf,
}

impl Projects {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("home").join(".claude").join("projects");
        let results = root.join("proj").join("sess").join(TOOL_RESULTS);
        fs::create_dir_all(&results).unwrap();
        Projects { tmp, root, results }
    }

    /// A harness-shaped persisted output file holding `content`.
    fn genuine(&self, content: &str) -> PathBuf {
        let path = self.results.join("out.txt");
        fs::write(&path, content).unwrap();
        path
    }

    /// A file outside the projects root, inside its own `tool-results` dir,
    /// so only the root check can refuse it.
    fn outside(&self) -> PathBuf {
        let dir = self.tmp.path().join("outside").join(TOOL_RESULTS);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.txt");
        fs::write(&path, "forged").unwrap();
        path
    }

    fn refusal(&self, path: &Path) -> String {
        format!("{:#}", read_persisted(path, &self.root).unwrap_err())
    }
}

#[test]
fn inline_text_reads_both_payload_shapes() {
    let payload = json!({
        "tool_response": {"stdout": "out", "stderr": "err"},
        "tool_result": {"output": "legacy"},
    });
    assert_eq!(inline_text(&payload), "out\nerr\nlegacy");
}

#[test]
fn a_genuine_file_named_by_the_structured_field_is_read() {
    let projects = Projects::new();
    let path = projects.genuine("full output\n");
    let payload = json!({"tool_response": {"stdout": "preview", "persistedOutputPath": path}});

    let collected = collect(&payload, &projects.root);
    assert_eq!(collected.text, "preview\nfull output\n");
    assert!(collected.persisted_refusal.is_none());
}

#[test]
fn a_genuine_file_named_by_the_inline_wrapper_is_read() {
    let projects = Projects::new();
    let path = projects.genuine("full output\n");
    let stdout = format!(
        "<persisted-output>\nOutput too large (43.5KB). {SAVED_TO}{}\n</persisted-output>",
        path.display()
    );

    let collected = collect(&json!({"tool_result": {"output": stdout}}), &projects.root);
    assert!(
        collected.text.ends_with("\nfull output\n"),
        "{}",
        collected.text
    );
    assert!(collected.persisted_refusal.is_none());
}

#[test]
fn a_path_outside_the_projects_root_is_refused() {
    let projects = Projects::new();
    let path = projects.outside();
    assert!(projects.refusal(&path).contains("does not resolve under"));
}

#[test]
fn a_dotdot_traversal_out_of_the_projects_root_is_refused() {
    let projects = Projects::new();
    projects.outside();
    let path = projects
        .root
        .join("../../../outside")
        .join(TOOL_RESULTS)
        .join("out.txt");
    assert!(projects.refusal(&path).contains(".."));
}

#[test]
fn a_symlinked_file_is_refused() {
    let projects = Projects::new();
    let outside = projects.outside();
    let link = projects.results.join("link.txt");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    assert!(projects.refusal(&link).contains("no-follow open failed"));
}

#[test]
fn a_symlinked_directory_leading_out_of_the_root_is_refused() {
    let projects = Projects::new();
    let outside = projects.outside();
    let session = projects.root.join("proj").join("other-session");
    fs::create_dir_all(&session).unwrap();
    std::os::unix::fs::symlink(outside.parent().unwrap(), session.join(TOOL_RESULTS)).unwrap();

    let path = session.join(TOOL_RESULTS).join("out.txt");
    assert!(projects.refusal(&path).contains("does not resolve under"));
}

#[test]
fn a_file_outside_a_tool_results_directory_is_refused() {
    let projects = Projects::new();
    let path = projects.root.join("proj").join("notes.txt");
    fs::write(&path, "x").unwrap();
    assert!(projects.refusal(&path).contains(TOOL_RESULTS));
}

#[test]
fn a_fifo_is_refused_without_blocking() {
    let projects = Projects::new();
    let fifo = projects.results.join("pipe");
    let c_path = CString::new(fifo.as_os_str().as_bytes()).unwrap();
    // SAFETY: `c_path` is a valid NUL-terminated path for the call's duration.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    assert!(projects.refusal(&fifo).contains("not a regular file"));
}

#[test]
fn a_directory_is_refused() {
    let projects = Projects::new();
    let dir = projects.results.join("nested");
    fs::create_dir(&dir).unwrap();
    assert!(projects.refusal(&dir).contains("not a regular file"));
}

#[test]
fn an_oversized_file_is_refused() {
    let projects = Projects::new();
    let path = projects.results.join("big.txt");
    fs::File::create(&path)
        .unwrap()
        .set_len(MAX_PERSISTED_OUTPUT_BYTES + 1)
        .unwrap();
    assert!(projects.refusal(&path).contains("cap"));
}

#[test]
fn a_relative_path_is_refused() {
    let projects = Projects::new();
    assert!(projects
        .refusal(Path::new("tool-results/out.txt"))
        .contains("not absolute"));
}

#[test]
fn a_refused_file_is_reported_and_its_content_left_out() {
    let projects = Projects::new();
    let path = projects.outside();
    let payload = json!({"tool_response": {"stdout": "preview", "persistedOutputPath": path}});

    let collected = collect(&payload, &projects.root);
    assert_eq!(collected.text, "preview");
    assert!(collected
        .persisted_refusal
        .unwrap()
        .contains("was not read"));
}
