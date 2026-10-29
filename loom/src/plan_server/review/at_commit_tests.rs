//! Tests for reads at a commit: discovery's name rules, git's statuses and the size limit.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;

use super::compare::{
    changed_since, discoverable, parse_name_status, text_at, BlobTooLarge, ChangedDocChange,
};
use super::git_tests::{commit, doc, git, init, put, repo, BEFORE};
use crate::plan_server::files::doc::MAX_EDIT_BYTES;
use crate::plan_server::start_with_routes;
use crate::plan_server::tests::support::{self, bootstrap, get, json_body, skip, status};

#[test]
fn discoverable_applies_the_walks_skip_rules_to_every_component() {
    for listed in ["doc/a.md", "doc/product/section_01.md", "doc/loom/notes.md"] {
        assert!(discoverable(Path::new(listed)), "{listed}");
    }
    for skipped in [
        "doc/.env",
        "doc/.secret/x.md",
        "doc/a.md~",
        "doc/drafts/a.md.swp",
        "doc/loom/summaries/a.json",
    ] {
        assert!(!discoverable(Path::new(skipped)), "{skipped}");
    }
}

#[test]
fn at_commit_and_compare_follow_discovery() -> Result<()> {
    if skip("at_commit_and_compare_follow_discovery") {
        return Ok(());
    }
    let dir = support::project();
    let root = dir.path();
    init(root);
    put(root, "doc/.env", "SECRET=committed\n");
    put(root, "doc/.secret/x.md", "# Secret committed\n");
    put(root, "doc/removed.md", "# Removed\n");
    let head = commit(root, BEFORE);
    fs::remove_file(root.join("doc/removed.md"))?;
    put(root, "doc/.env", "SECRET=edited\n");
    put(root, "doc/.secret/x.md", "# Secret edited\n");
    put(root, "doc/notes.md~", "backup\n");
    let server = start_with_routes(support::options(&dir), super::routes())?;
    let cookie = bootstrap(&server);
    let at = |path: &str| {
        get(
            &server,
            &format!("/api/at-commit?commit={head}&path={path}"),
            &cookie,
        )
    };

    for path in ["doc/.env", "doc/.secret/x.md"] {
        let response = at(path);
        assert_eq!(status(&response), 404, "{path}: {response}");
        assert!(!response.contains("committed"), "{path}: {response}");
    }
    let removed = at("doc/removed.md");
    assert_eq!(status(&removed), 200, "{removed}");
    assert_eq!(
        json_body(&removed),
        json!({"path": "doc/removed.md", "text": "# Removed\n"}),
        "a document removed since the commit is still read there"
    );
    let compare = get(&server, &format!("/api/compare?commit={head}"), &cookie);
    assert_eq!(status(&compare), 200, "{compare}");
    let body = json_body(&compare);
    let docs = body["docs"].as_array().context("compare docs")?;
    assert!(docs.contains(&json!({"path": "doc/removed.md", "change": "removed"})));
    for listed in docs {
        let path = listed["path"].as_str().unwrap_or_default();
        assert!(discoverable(Path::new(path)), "compare listed {listed}");
    }
    server.stop()?;
    Ok(())
}

#[test]
fn unmerged_status_is_changed_and_non_utf8_paths_are_skipped() {
    let records = b"U\0doc/a.md\0M\0doc/bad-\xff.md\0D\0doc/c.md\0";
    let docs = parse_name_status(records).expect("an unmerged path parses");
    assert_eq!(
        docs,
        vec![
            doc("doc/a.md", ChangedDocChange::Changed),
            doc("doc/c.md", ChangedDocChange::Removed),
        ]
    );
}

#[test]
fn a_path_git_lists_twice_keeps_its_first_entry() {
    // During a merge git can print an unmerged path twice; the later status never replaces it.
    let records = b"U\0doc/a.md\0D\0doc/b.md\0D\0doc/a.md\0A\0doc/c.md\0M\0doc/b.md\0";
    let docs = parse_name_status(records).expect("a repeated path parses");
    assert_eq!(
        docs,
        vec![
            doc("doc/a.md", ChangedDocChange::Changed),
            doc("doc/b.md", ChangedDocChange::Removed),
            doc("doc/c.md", ChangedDocChange::Added),
        ]
    );
}

#[cfg(unix)]
#[test]
fn untracked_non_utf8_name_does_not_fail_compare() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let repo = repo();
    put(&repo.root, "doc/a.md", "# A\n");
    let head = commit(&repo.root, BEFORE);
    let bad = repo
        .root
        .join("doc")
        .join(OsStr::from_bytes(b"bad-\xff.md"));
    fs::write(bad, "# Bad\n").expect("write a non-UTF-8 name");
    put(&repo.root, "doc/a.md", "# A\n\nEdited.\n");

    let docs = changed_since(&repo.root, &head).expect("a non-UTF-8 name is skipped");

    assert_eq!(docs, vec![doc("doc/a.md", ChangedDocChange::Changed)]);
}

#[test]
fn text_at_refuses_a_blob_over_the_file_mode_limit() {
    let repo = repo();
    put(&repo.root, "doc/big.md", &"x".repeat(MAX_EDIT_BYTES + 1));
    put(&repo.root, "doc/edge.md", &"x".repeat(MAX_EDIT_BYTES));
    let head = commit(&repo.root, BEFORE);

    let error = text_at(&repo.root, &head, "big.md").expect_err("a blob over 1 MiB is refused");
    assert!(error.is::<BlobTooLarge>(), "{error:#}");
    let edge = text_at(&repo.root, &head, "edge.md").expect("a 1 MiB blob reads");
    assert_eq!(edge.map(|text| text.len()), Some(MAX_EDIT_BYTES));
}

#[test]
fn text_at_a_directory_or_submodule_entry_is_none() {
    let repo = repo();
    put(&repo.root, "doc/product/a.md", "# A\n");
    git(&repo.root, &["add", "-A"], None);
    // A gitlink whose commit is not in this repository, as a submodule entry is.
    let gitlink = format!("160000,{},doc/sub", "1".repeat(40));
    git(
        &repo.root,
        &["update-index", "--add", "--cacheinfo", &gitlink],
        None,
    );
    git(&repo.root, &["commit", "-q", "-m", "fixture"], Some(BEFORE));
    let head = git(&repo.root, &["rev-parse", "HEAD"], None);

    for path in ["product", "sub"] {
        let text = text_at(&repo.root, &head, path)
            .unwrap_or_else(|error| panic!("{path} is not an error: {error:#}"));
        assert_eq!(text, None, "{path}");
    }
    let file = text_at(&repo.root, &head, "product/a.md").expect("a file reads");
    assert_eq!(file.as_deref(), Some("# A\n"));
}
