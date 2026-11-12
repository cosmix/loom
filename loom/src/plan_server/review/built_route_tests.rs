//! Tests for `/api/built`, which takes the doc-relative document path.

use std::os::unix::fs::symlink;

use anyhow::Result;
use serde_json::json;

use super::route_tests::{served, SECTION};
use crate::plan_server::tests::support::{bootstrap, get, json_body, skip, status};

/// The web sends the document path the other document routes take (`product/demo/section_01.md`);
/// a DocId (no `.md`) names no document here and answers no notes.
#[test]
fn built_takes_the_doc_relative_document_path() -> Result<()> {
    if skip("built_takes_the_doc_relative_document_path") {
        return Ok(());
    }
    let (_dir, server) = served(&[
        (SECTION, "# One\n"),
        ("doc/product/other/section_01.md", "# Other\n"),
        (
            "doc/plans/demo/IN_PROGRESS-PLAN-demo.md",
            "# Demo\nImplements: section_01\n",
        ),
    ])?;
    let cookie = bootstrap(&server);

    let by_path = get(
        &server,
        "/api/built?section=product%2Fdemo%2Fsection_01.md",
        &cookie,
    );
    let by_id = get(
        &server,
        "/api/built?section=product%2Fdemo%2Fsection_01",
        &cookie,
    );
    let other_feature = get(
        &server,
        "/api/built?section=product%2Fother%2Fsection_01.md",
        &cookie,
    );

    assert_eq!(status(&by_path), 200, "{by_path}");
    assert_eq!(
        json_body(&by_path),
        json!({"notes": [{"kind": "running", "slug": "demo"}]})
    );
    assert_eq!(json_body(&by_id), json!({"notes": []}));
    assert_eq!(json_body(&other_feature), json!({"notes": []}));
    server.stop()?;
    Ok(())
}

#[test]
fn built_rejects_a_path_that_leaves_the_doc_tree() -> Result<()> {
    if skip("built_rejects_a_path_that_leaves_the_doc_tree") {
        return Ok(());
    }
    let (_dir, server) = served(&[(SECTION, "# One\n")])?;
    let cookie = bootstrap(&server);

    let response = get(&server, "/api/built?section=..%2Fx.md", &cookie);

    assert_eq!(status(&response), 400, "{response}");
    assert_eq!(json_body(&response)["error"], "bad_path");
    server.stop()?;
    Ok(())
}

#[test]
fn built_rejects_a_path_through_a_symlink() -> Result<()> {
    if skip("built_rejects_a_path_through_a_symlink") {
        return Ok(());
    }
    let (dir, server) = served(&[(SECTION, "# One\n")])?;
    symlink("demo", dir.path().join("doc/product/linked"))?;
    let cookie = bootstrap(&server);

    let linked = get(
        &server,
        "/api/built?section=product%2Flinked%2Fsection_01.md",
        &cookie,
    );
    let plain = get(
        &server,
        "/api/built?section=product%2Fdemo%2Fsection_01.md",
        &cookie,
    );

    assert_eq!(status(&linked), 400, "{linked}");
    assert_eq!(json_body(&linked)["error"], "bad_path");
    assert_eq!(status(&plain), 200, "{plain}");
    server.stop()?;
    Ok(())
}
