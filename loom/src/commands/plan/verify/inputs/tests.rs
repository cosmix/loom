use super::input_findings;
use crate::commands::plan::verify::execute;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const PLAN: &str = "doc/plans/PLAN-foo.md";
const BRIEF: &str = "doc/plans/briefs/foo/a.md";

fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join("absent-global"))
        .env("GIT_CONFIG_SYSTEM", root.join("absent-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A git repo with one seed commit and the plan written but not committed.
fn repo(plan_text: &str) -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test"),
        ("user.email", "test@example.invalid"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        git(root, &["config", key, value]);
    }
    write(root, "seed", "seed");
    git(root, &["add", "seed"]);
    git(root, &["commit", "-m", "seed"]);
    write(root, PLAN, plan_text);
    temp
}

fn messages(errors: &[crate::commands::plan::verify::JsonError]) -> Vec<&str> {
    errors.iter().map(|e| e.message.as_str()).collect()
}

#[test]
fn invalid_glob_token_is_a_hard_error() {
    let temp = repo("Read doc/plans/briefs/x**.md");
    let (errors, _) = input_findings(temp.path(), &temp.path().join(PLAN));
    let text = messages(&errors);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(text[0].starts_with("doc/plans/briefs/x**.md (referenced in doc/plans/PLAN-foo.md)"));
}

#[test]
fn missing_brief_is_a_hard_error_naming_the_source() {
    let temp = repo(&format!("Read {BRIEF}"));
    let (errors, notes) = input_findings(temp.path(), &temp.path().join(PLAN));
    let text = messages(&errors);
    assert_eq!(text.len(), 1, "{text:?}");
    assert!(
        text[0].contains(BRIEF) && text[0].contains("missing file"),
        "{text:?}"
    );
    assert!(
        text[0].contains("referenced in doc/plans/PLAN-foo.md"),
        "{text:?}"
    );
    // The plan itself is uncommitted, so it is the only note.
    assert_eq!(notes.len(), 1, "{notes:?}");
}

#[test]
fn uncommitted_present_brief_is_a_note_not_an_error() {
    let temp = repo(&format!("Read {BRIEF}"));
    write(temp.path(), BRIEF, "brief");
    let (errors, notes) = input_findings(temp.path(), &temp.path().join(PLAN));
    assert!(errors.is_empty(), "{:?}", messages(&errors));
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].starts_with("2 plan input(s) not committed"),
        "{notes:?}"
    );
    assert!(
        notes[0].contains(BRIEF) && notes[0].contains(PLAN),
        "{notes:?}"
    );
    assert!(notes[0].contains("loom run refuses"), "{notes:?}");
}

#[test]
fn committed_inputs_produce_no_findings() {
    let temp = repo(&format!("Read {BRIEF}"));
    write(temp.path(), BRIEF, "brief");
    git(temp.path(), &["add", "doc"]);
    git(temp.path(), &["commit", "-m", "plan"]);
    let (errors, notes) = input_findings(temp.path(), &temp.path().join(PLAN));
    assert!(errors.is_empty() && notes.is_empty(), "{notes:?}");
}

#[test]
fn prose_after_a_code_span_is_not_a_reference() {
    let temp = repo("**Briefs live outside `doc/plans/briefs/`.** Nothing else.");
    let (errors, _) = input_findings(temp.path(), &temp.path().join(PLAN));
    assert!(errors.is_empty(), "{:?}", messages(&errors));
}

#[test]
fn outside_a_git_repository_the_note_is_skipped() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), PLAN, &format!("Read {BRIEF}"));
    write(temp.path(), BRIEF, "brief");
    let (errors, notes) = input_findings(temp.path(), &temp.path().join(PLAN));
    assert!(errors.is_empty() && notes.is_empty(), "{notes:?}");
}

fn plan_with_reference(reference: &str) -> String {
    format!(
        "# Inputs\n\n{reference}\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  \
         stages:\n    - id: stage-one\n      name: \"Stage One\"\n      summary: \"Does it.\"\n      \
         stage_type: standard\n      working_dir: \".\"\n      acceptance:\n        \
         - \"rg -q present notes.txt\"\n```\n\n<!-- END loom METADATA -->\n"
    )
}

#[test]
fn verify_fails_without_strict_on_an_invalid_glob() {
    let temp = repo(&plan_with_reference("Read doc/plans/briefs/x**.md"));
    let error = execute(&temp.path().join(PLAN), false, false, true).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Plan validation failed (1 error(s)"),
        "{error}"
    );
}

#[test]
fn verify_passes_with_an_uncommitted_present_brief() {
    let temp = repo(&plan_with_reference(&format!("Read {BRIEF}")));
    write(temp.path(), BRIEF, "brief");
    // The note is not a warning: under --strict only the fixture's own warnings
    // (no distill stage, no knowledge tree) count, never the uncommitted inputs.
    execute(&temp.path().join(PLAN), false, false, true).unwrap();
    let strict = execute(&temp.path().join(PLAN), true, false, true).unwrap_err();
    assert!(
        strict.to_string().contains("(0 error(s), 3 warning(s))"),
        "{strict}"
    );
}
