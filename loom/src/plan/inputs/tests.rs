use super::{collect_inputs, Origin};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const PLAN: &str = "doc/plans/PLAN-foo.md";

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn plan_with_invalid_glob_names_token_and_plan() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), PLAN, "Read doc/plans/briefs/x**.md now.");
    let inputs = collect_inputs(temp.path(), Path::new(PLAN)).unwrap();
    assert_eq!(inputs.invalid.len(), 1, "{:?}", inputs.invalid);
    let text = inputs.invalid[0].to_string();
    assert!(
        text.starts_with("doc/plans/briefs/x**.md (referenced in doc/plans/PLAN-foo.md): "),
        "{text}"
    );
    assert!(
        text.contains(
            "not a valid path or glob: recursive wildcards must form a single path component"
        ),
        "{text}"
    );
}

#[test]
fn brief_with_invalid_glob_names_the_brief_as_source() {
    let temp = TempDir::new().unwrap();
    let brief = "doc/plans/briefs/foo/a.md";
    write(temp.path(), PLAN, &format!("See {brief}"));
    write(temp.path(), brief, "Also doc/plans/briefs/y**.md");
    let inputs = collect_inputs(temp.path(), Path::new(PLAN)).unwrap();
    assert_eq!(inputs.invalid.len(), 1, "{:?}", inputs.invalid);
    assert_eq!(inputs.invalid[0].origin, Origin::File(PathBuf::from(brief)));
    assert!(inputs.invalid[0]
        .to_string()
        .contains(&format!("referenced in {brief}")));
}

#[test]
fn parent_dir_and_symlink_are_invalid_references_not_errors() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        PLAN,
        "doc/plans/briefs/../../secret.md doc/plans/briefs/link.md doc/plans/briefs/zzz**.md",
    );
    write(root, "real.md", "x");
    fs::create_dir_all(root.join("doc/plans/briefs")).unwrap();
    std::os::unix::fs::symlink(root.join("real.md"), root.join("doc/plans/briefs/link.md"))
        .unwrap();
    let inputs = collect_inputs(root, Path::new(PLAN)).unwrap();
    let reasons: Vec<&str> = inputs.invalid.iter().map(|r| r.reason.as_str()).collect();
    assert_eq!(reasons.len(), 3, "{reasons:?}");
    assert!(reasons.iter().any(|r| r.contains("parent directories")));
    assert!(reasons.iter().any(|r| r.contains("symlink")));
    assert!(reasons
        .iter()
        .any(|r| r.contains("not a valid path or glob")));
}

#[test]
fn missing_reference_stays_an_input_with_its_origin() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), PLAN, "doc/plans/briefs/foo/gone.md");
    let inputs = collect_inputs(temp.path(), Path::new(PLAN)).unwrap();
    assert!(inputs.invalid.is_empty());
    let gone = PathBuf::from("doc/plans/briefs/foo/gone.md");
    assert_eq!(inputs.files[&gone], Origin::File(PathBuf::from(PLAN)));
}

#[test]
fn bundle_members_carry_the_bundle_origin() {
    let temp = TempDir::new().unwrap();
    write(temp.path(), PLAN, "no references");
    write(temp.path(), "doc/plans/briefs/foo/w.md", "worker");
    let inputs = collect_inputs(temp.path(), Path::new(PLAN)).unwrap();
    assert_eq!(
        inputs.files[Path::new("doc/plans/briefs/foo/w.md")],
        Origin::Bundle
    );
}
