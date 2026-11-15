use super::*;
use crate::process::ProcessTimeoutError;
use crate::test_git::{git, TestRepo};
use std::collections::BTreeSet;
use std::fs;
use tempfile::TempDir;

#[test]
fn timeout_is_reported_as_a_resource_limit() {
    let error = anyhow::Error::new(ProcessTimeoutError::new(
        "git status",
        std::time::Duration::from_secs(1),
    ));

    assert_eq!(
        classify_git_error(error),
        EvidenceUnavailableReason::ResourceLimit
    );
}

#[test]
fn nul_status_parser_orders_rename_destination_before_source() {
    let mut fields = b"R  src/new name.rs\0src/old name.rs\0".split(|byte| *byte == b'\0');
    let record = fields.next().unwrap();

    assert_eq!(
        status_rename_paths(record, &mut fields),
        Some(("src/new name.rs".as_bytes(), "src/old name.rs".as_bytes()))
    );
}

#[test]
fn literal_pathspec_selects_only_magic_named_source() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let source = ":(glob)**";
    fs::write(root.join(source), "first\n").unwrap();
    fs::write(root.join("unrelated.rs"), "first\n").unwrap();
    TestRepo::builder().no_seed().init_in(root);
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "initial"]);
    let verified = git(root, &["rev-parse", "HEAD"]);
    fs::write(root.join(source), "changed\n").unwrap();
    fs::write(root.join("unrelated.rs"), "changed\n").unwrap();

    let key = EvidenceKey {
        verified,
        sources: BTreeSet::from([source.to_string()]),
    };
    assert_eq!(
        working_tree_paths(root, &key).unwrap(),
        BTreeSet::from([source.to_string()])
    );
}
