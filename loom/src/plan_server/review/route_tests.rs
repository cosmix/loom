//! Tests for the review HTTP routes.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::json;
use tempfile::TempDir;

use crate::plan_server::authoring::Take;
use crate::plan_server::files::doc::DONE_NOTICE;
use crate::plan_server::files::texts::{TextStore, TurnId};
use crate::plan_server::gate::browser_cookie_name;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::start_with_routes;
use crate::plan_server::tests::support::{
    self, bootstrap, cookie_pair, get, json_body, post_json, skip, status,
};
use crate::plan_server::wire::review::ImplementedByResponse;
use crate::plan_server::RunningServer;

const MISSING_COMMIT: &str = "1111111111111111111111111111111111111111";
pub(super) const SECTION: &str = "doc/product/demo/section_01.md";
const END_TEXT: &str = "# Turn end\n";
const INVALID_PLAN: &str = "# Invalid plan\n\n<!-- loom METADATA -->\n\n```yaml\nloom: [\n```\n\n<!-- END loom METADATA -->\n";

fn write(dir: &TempDir, rel: &str, text: &str) -> Result<()> {
    let path = dir.path().join(rel);
    fs::create_dir_all(path.parent().context("fixture path has a parent")?)?;
    fs::write(path, text)?;
    Ok(())
}

pub(super) fn served(files: &[(&str, &str)]) -> Result<(TempDir, RunningServer)> {
    let dir = support::project();
    // An unborn Git repository needs only HEAD, objects and refs; no history is used here.
    fs::create_dir_all(dir.path().join(".git/objects"))?;
    fs::create_dir_all(dir.path().join(".git/refs/heads"))?;
    write(&dir, ".git/HEAD", "ref: refs/heads/main\n")?;
    write(
        &dir,
        ".git/config",
        "[core]\nrepositoryformatversion = 0\nbare = false\n",
    )?;
    for (rel, text) in files {
        write(&dir, rel, text)?;
    }
    let server = start_with_routes(support::options(&dir), super::routes())?;
    Ok((dir, server))
}

fn take_authoring(server: &RunningServer, cookie: &str) -> Result<()> {
    let name = browser_cookie_name(server.local.port());
    let pair = cookie_pair(cookie, &name).context("bootstrap supplied a browser cookie")?;
    let (_, value) = pair.split_once('=').context("cookie has a value")?;
    let browser = BrowserId::parse(value).context("valid browser id")?;
    server
        .ctx
        .require_authoring(&browser, Take::TakeIfFree)
        .map_err(|refusal| anyhow::anyhow!("take authoring: {refusal:?}"))?;
    Ok(())
}

fn pin_turn(dir: &TempDir, server: &RunningServer, path: &str) -> Result<()> {
    let store = server
        .ctx
        .extensions()
        .get::<TextStore>()
        .context("text store")?;
    let key = TurnId("session/turn".to_owned());
    store.begin_turn(&key);
    write(dir, path, END_TEXT)?;
    let changes = store.end_turn(&key, dir.path(), &server.ctx.project());
    assert!(changes.iter().any(|change| change.path == Path::new(path)));
    assert!(store.turn_pin(&key, Path::new(path)).is_some());
    assert!(store.turn_end(&key, Path::new(path)).is_some());
    Ok(())
}

fn restore(server: &RunningServer, cookie: &str, path: &str, turn: &str) -> String {
    let body = json!({
        "session": "session", "turn": turn, "paths": [path], "force": false,
    });
    post_json(server, "/api/restore", cookie, &body.to_string())
}

#[test]
fn restore_refused_for_a_non_holder_by_the_gate() -> Result<()> {
    if skip("restore_refused_for_a_non_holder_by_the_gate") {
        return Ok(());
    }
    let (dir, server) = served(&[(SECTION, "# Before\n")])?;
    pin_turn(&dir, &server, SECTION)?;
    let holder = bootstrap(&server);
    take_authoring(&server, &holder)?;
    let other = bootstrap(&server);

    let response = restore(&server, &other, SECTION, "turn");

    assert_eq!(status(&response), 403, "{response}");
    assert_eq!(json_body(&response)["error"], "not_holder");
    assert_eq!(fs::read_to_string(dir.path().join(SECTION))?, END_TEXT);
    server.stop()?;
    Ok(())
}

#[test]
fn unknown_turn_404() -> Result<()> {
    if skip("unknown_turn_404") {
        return Ok(());
    }
    let (_dir, server) = served(&[(SECTION, "# Before\n")])?;
    let cookie = bootstrap(&server);
    take_authoring(&server, &cookie)?;

    let response = restore(&server, &cookie, SECTION, "unknown");

    assert_eq!(status(&response), 404, "{response}");
    assert_eq!(json_body(&response)["error"], "not_found");
    server.stop()?;
    Ok(())
}

#[test]
fn restore_of_a_locked_plan_is_refused_by_save_document() -> Result<()> {
    if skip("restore_of_a_locked_plan_is_refused_by_save_document") {
        return Ok(());
    }
    let path = "doc/plans/DONE-PLAN-old.md";
    let (dir, server) = served(&[(path, "# Before\n")])?;
    pin_turn(&dir, &server, path)?;
    let cookie = bootstrap(&server);
    take_authoring(&server, &cookie)?;

    let response = restore(&server, &cookie, path, "turn");

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({"files": [{
            "path": path, "result": "refused", "reason": DONE_NOTICE,
        }]}),
    );
    assert_eq!(fs::read_to_string(dir.path().join(path))?, END_TEXT);
    server.stop()?;
    Ok(())
}

#[test]
fn restore_of_a_path_the_turn_did_not_change_says_so() -> Result<()> {
    if skip("restore_of_a_path_the_turn_did_not_change_says_so") {
        return Ok(());
    }
    let changed = "doc/plans/PLAN-changed.md";
    let (dir, server) = served(&[(changed, "# Before\n"), (SECTION, "# Untouched\n")])?;
    pin_turn(&dir, &server, changed)?;
    let cookie = bootstrap(&server);
    take_authoring(&server, &cookie)?;

    let response = restore(&server, &cookie, SECTION, "turn");

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({"files": [{
            "path": SECTION, "result": "refused", "reason": "not changed by this turn",
        }]}),
    );
    assert_eq!(
        fs::read_to_string(dir.path().join(SECTION))?,
        "# Untouched\n"
    );
    server.stop()?;
    Ok(())
}

#[test]
fn invalid_maps_to_invalid() -> Result<()> {
    if skip("invalid_maps_to_invalid") {
        return Ok(());
    }
    let path = "doc/plans/PLAN-invalid.md";
    let (dir, server) = served(&[(path, INVALID_PLAN)])?;
    pin_turn(&dir, &server, path)?;
    let cookie = bootstrap(&server);
    take_authoring(&server, &cookie)?;

    let response = restore(&server, &cookie, path, "turn");

    assert_eq!(status(&response), 200, "{response}");
    let body = json_body(&response);
    let files = body["files"].as_array().context("restore files")?;
    assert_eq!(files.len(), 1, "{body}");
    assert_eq!(files[0]["path"], path);
    assert_eq!(files[0]["result"], "invalid", "{body}");
    let errors = files[0]["errors"].as_array().context("validation errors")?;
    assert!(!errors.is_empty(), "{body}");
    assert!(errors
        .iter()
        .all(|error| error.as_str().is_some_and(|s| !s.is_empty())));
    assert_eq!(fs::read_to_string(dir.path().join(path))?, END_TEXT);
    server.stop()?;
    Ok(())
}

#[test]
fn ref_option_like_400() -> Result<()> {
    if skip("ref_option_like_400") {
        return Ok(());
    }
    let (_dir, server) = served(&[])?;
    let cookie = bootstrap(&server);

    let response = get(&server, "/api/ref?name=--output=x", &cookie);

    assert_eq!(status(&response), 400, "{response}");
    assert_eq!(json_body(&response)["error"], "bad_ref");
    server.stop()?;
    Ok(())
}

#[test]
fn compare_non_hex_400() -> Result<()> {
    if skip("compare_non_hex_400") {
        return Ok(());
    }
    let (_dir, server) = served(&[])?;
    let cookie = bootstrap(&server);
    let non_hex = "g".repeat(40);

    let response = get(&server, &format!("/api/compare?commit={non_hex}"), &cookie);

    assert_eq!(status(&response), 400, "{response}");
    assert_eq!(json_body(&response)["error"], "bad_commit");
    server.stop()?;
    Ok(())
}

#[test]
fn compare_available_false() -> Result<()> {
    if skip("compare_available_false") {
        return Ok(());
    }
    let (_dir, server) = served(&[(SECTION, "# Existing document\n")])?;
    let cookie = bootstrap(&server);

    let response = get(
        &server,
        &format!("/api/compare?commit={MISSING_COMMIT}"),
        &cookie,
    );

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({"commit": MISSING_COMMIT, "available": false, "docs": []}),
    );
    server.stop()?;
    Ok(())
}

#[test]
fn at_commit_dot_dot_400() -> Result<()> {
    if skip("at_commit_dot_dot_400") {
        return Ok(());
    }
    let (_dir, server) = served(&[])?;
    let cookie = bootstrap(&server);

    let response = get(
        &server,
        &format!("/api/at-commit?commit={MISSING_COMMIT}&path=doc/../x"),
        &cookie,
    );

    assert_eq!(status(&response), 400, "{response}");
    // This must fail the path check before the unavailable commit check.
    assert_eq!(json_body(&response)["error"], "bad_path");
    server.stop()?;
    Ok(())
}

#[test]
fn built_with_no_plans_is_empty() -> Result<()> {
    if skip("built_with_no_plans_is_empty") {
        return Ok(());
    }
    let (_dir, server) = served(&[(SECTION, "# Section one\n")])?;
    let cookie = bootstrap(&server);

    let response = get(
        &server,
        "/api/built?section=product%2Fdemo%2Fsection_01.md",
        &cookie,
    );

    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(json_body(&response), json!({"notes": []}));
    server.stop()?;
    Ok(())
}

#[test]
fn built_accepts_feature_section_ids_and_scopes_the_plans() -> Result<()> {
    if skip("built_accepts_feature_section_ids_and_scopes_the_plans") {
        return Ok(());
    }
    let (_dir, server) = served(&[
        (SECTION, "# One\n"),
        ("doc/product/other/section_01.md", "# Other\n"),
        (
            "doc/plans/demo/IN_PROGRESS-PLAN-demo.md",
            "# Demo\nImplements: section_01\n",
        ),
        (
            "doc/plans/other/IN_PROGRESS-PLAN-other.md",
            "# Other\nImplements: section_01\n",
        ),
    ])?;
    let cookie = bootstrap(&server);
    let response = get(
        &server,
        "/api/built?section=product%2Fdemo%2Fsection_01.md",
        &cookie,
    );
    assert_eq!(status(&response), 200, "{response}");
    assert_eq!(
        json_body(&response),
        json!({"notes": [{"kind": "running", "slug": "demo"}]})
    );
    server.stop()?;
    Ok(())
}

#[test]
fn implementing_plans_are_keyed_by_feature_section_doc_id() -> Result<()> {
    if skip("implementing_plans_are_keyed_by_feature_section_doc_id") {
        return Ok(());
    }
    let (_dir, server) = served(&[
        (SECTION, "# One\n"),
        ("doc/product/other/section_01.md", "# Other\n"),
        (
            "doc/plans/demo/PLAN-demo.md",
            "# Demo\nImplements: section_01\n",
        ),
        (
            "doc/plans/other/PLAN-other.md",
            "# Other\nImplements: section_01\n",
        ),
    ])?;
    let cookie = bootstrap(&server);
    let response = get(&server, "/api/implemented-by", &cookie);
    assert_eq!(status(&response), 200, "{response}");
    let body: ImplementedByResponse = serde_json::from_value(json_body(&response))?;
    assert_eq!(body.sections.len(), 2);
    assert_eq!(body.sections["product/demo/section_01"][0].slug, "demo");
    assert_eq!(body.sections["product/other/section_01"][0].slug, "other");
    assert_eq!(body.sections["product/demo/section_01"].len(), 1);
    assert_eq!(body.sections["product/other/section_01"].len(), 1);
    server.stop()?;
    Ok(())
}
