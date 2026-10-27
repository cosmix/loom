//! The file-mode HTTP routes and the batch broadcasts, over a started server.

use std::path::Path;

use serde_json::Value;
use tempfile::TempDir;

use super::validate::{plan, SELF_DEPENDENCY};
use crate::plan_server::files::validate::ValidationCache;
use crate::plan_server::tests::support::{
    bootstrap, connect_ws, get, json_body, origin, read_until, request, send, serve, skip, status,
};
use crate::plan_server::RunningServer;
use crate::planning::sections::normalized_hash;

/// The support project (`doc/product/index.md`) plus `files`.
fn project_with(files: &[(&str, &str)]) -> TempDir {
    let dir = crate::plan_server::tests::support::project();
    for (rel, text) in files {
        write(&dir, rel, text);
    }
    dir
}

fn write(dir: &TempDir, rel: &str, text: &str) {
    let path = dir.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `POST /api/save` of `text` for `path` (relative to `doc/`) opened at `opened`'s hash.
fn post_save(server: &RunningServer, cookie: &str, query: &str, text: &str) -> String {
    let origin = origin(server);
    let extra = [origin.as_str(), "Content-Type: text/plain; charset=utf-8"];
    let target = format!("/api/save?{query}");
    send(
        server.local,
        request("POST", server, &target, cookie, &extra, text).as_bytes(),
    )
}

fn query(path: &str, opened: &str) -> String {
    format!("path={path}&hash={}", normalized_hash(opened))
}

#[test]
fn save_answers_saved_conflict_and_invalid() {
    if skip("save_answers_saved_conflict_and_invalid") {
        return;
    }
    let valid = plan(".", "true", "");
    let dir = project_with(&[("doc/plans/PLAN-x.md", &valid)]);
    let server = serve(&dir);
    let cookie = bootstrap(&server);

    let stale = post_save(
        &server,
        &cookie,
        &query("product/index.md", "# Old\n"),
        "# New\n",
    );
    assert_eq!(status(&stale), 409, "{stale}");
    let body = json_body(&stale);
    assert_eq!(body["result"], "conflict");
    assert_eq!(body["current_text"], "# Demo\n");

    let saved = post_save(
        &server,
        &cookie,
        &query("product/index.md", "# Demo\n"),
        "# New\n",
    );
    assert_eq!(status(&saved), 200, "{saved}");
    assert_eq!(json_body(&saved)["hash"], normalized_hash("# New\n"));

    let invalid = plan(".", "true", SELF_DEPENDENCY);
    let plan_query = query("plans/PLAN-x.md", &valid);
    let refused = post_save(&server, &cookie, &plan_query, &invalid);
    assert_eq!(status(&refused), 422, "{refused}");
    assert_eq!(json_body(&refused)["result"], "invalid");
    let forced = post_save(
        &server,
        &cookie,
        &format!("{plan_query}&with_errors=1"),
        &invalid,
    );
    assert_eq!(status(&forced), 200, "{forced}");
    assert_eq!(json_body(&forced)["validation"]["valid"], false);
    let on_disk = std::fs::read_to_string(dir.path().join("doc/plans/PLAN-x.md")).unwrap();
    assert_eq!(on_disk, invalid);
    server.stop().unwrap();
}

#[test]
fn save_refusals_map_to_statuses() {
    if skip("save_refusals_map_to_statuses") {
        return;
    }
    let dir = project_with(&[("doc/plans/DONE-PLAN-old.md", "# Old\n")]);
    let server = serve(&dir);
    let holder = bootstrap(&server);
    let other = bootstrap(&server);

    let locked = post_save(
        &server,
        &holder,
        &query("plans/DONE-PLAN-old.md", "# Old\n"),
        "# X\n",
    );
    assert_eq!(status(&locked), 403, "{locked}");
    assert_eq!(json_body(&locked)["error"], "locked");
    let missing = post_save(&server, &holder, &query("product/none.md", ""), "# X\n");
    assert_eq!(status(&missing), 404, "{missing}");
    assert!(!dir.path().join("doc/product/none.md").exists());
    let bad = post_save(&server, &holder, &query("../x.md", ""), "# X\n");
    assert_eq!(status(&bad), 400, "{bad}");

    // `holder` took authoring with its first save; another browser is refused by the gate.
    let refused = post_save(
        &server,
        &other,
        &query("product/index.md", "# Demo\n"),
        "# X\n",
    );
    assert_eq!(status(&refused), 403, "{refused}");
    let index = std::fs::read_to_string(dir.path().join("doc/product/index.md")).unwrap();
    assert_eq!(index, "# Demo\n");
    server.stop().unwrap();
}

#[test]
fn the_tree_route_answers_repo_relative_paths() {
    if skip("the_tree_route_answers_repo_relative_paths") {
        return;
    }
    let dir = project_with(&[("doc/plans/PLAN-x.md", &plan(".", "true", SELF_DEPENDENCY))]);
    let server = serve(&dir);
    let cookie = bootstrap(&server);
    let response = get(&server, "/api/tree", &cookie);
    assert_eq!(status(&response), 200, "{response}");
    let tree = json_body(&response);
    let paths = all_paths(&tree);
    assert!(
        paths.iter().any(|p| p == "doc/product/index.md"),
        "{paths:?}"
    );
    assert!(paths.iter().all(|p| p.starts_with("doc/")), "{paths:?}");
    let plan_entry = find_entry(&tree["plans"], "doc/plans/PLAN-x.md").expect("the plan entry");
    assert!(
        plan_entry["errors"].as_u64().is_some_and(|n| n > 0),
        "{plan_entry}"
    );
    server.stop().unwrap();
}

fn all_paths(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => {
            let own = map.get("path").and_then(Value::as_str).map(str::to_owned);
            own.into_iter()
                .chain(map.values().flat_map(all_paths))
                .collect()
        }
        Value::Array(items) => items.iter().flat_map(all_paths).collect(),
        _ => Vec::new(),
    }
}

fn find_entry<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let entries = value.as_array()?;
    entries.iter().find_map(|entry| {
        if entry["path"] == path {
            Some(entry)
        } else {
            find_entry(&entry["children"], path)
        }
    })
}

#[test]
fn the_cache_holds_unlocked_plans_only() {
    if skip("the_cache_holds_unlocked_plans_only") {
        return;
    }
    let valid = plan(".", "true", "");
    let dir = project_with(&[
        ("doc/plans/PLAN-x.md", &valid),
        ("doc/plans/DONE-PLAN-old.md", "# Old\n"),
        ("doc/plans/archive/PLAN-a.md", "# A\n"),
        ("doc/plans/notes.md", "# Notes\n"),
    ]);
    let server = serve(&dir);
    let cache = server
        .ctx
        .extensions()
        .get::<ValidationCache>()
        .expect("installed");
    let mut paths: Vec<_> = cache.lock().unwrap().keys().cloned().collect();
    paths.sort();
    assert_eq!(paths, vec![Path::new("doc/plans/PLAN-x.md").to_path_buf()]);
    server.stop().unwrap();
}

#[test]
fn batches_broadcast_doc_changed_validation_and_tree_changed() {
    if skip("batches_broadcast_doc_changed_validation_and_tree_changed") {
        return;
    }
    let dir = project_with(&[("doc/plans/PLAN-x.md", &plan(".", "true", ""))]);
    let server = serve(&dir);
    let cookie = bootstrap(&server);
    let origin = origin(&server).replacen("Origin: ", "", 1);
    let mut socket = connect_ws(&server, &cookie, Some(&origin)).expect("an admitted socket");
    read_until(&mut socket, "hello");

    let invalid = plan(".", "true", SELF_DEPENDENCY);
    write(&dir, "doc/plans/PLAN-x.md", &invalid);
    let changed: Value = serde_json::from_str(&read_until(&mut socket, "doc_changed")).unwrap();
    assert_eq!(changed["path"], "doc/plans/PLAN-x.md");
    assert_eq!(changed["hash"], normalized_hash(&invalid));
    let validation: Value = serde_json::from_str(&read_until(&mut socket, "validation")).unwrap();
    assert_eq!(validation["path"], "doc/plans/PLAN-x.md");
    assert_eq!(validation["validation"]["valid"], false);

    // The edit changed the plan's error count, so a `tree_changed` may precede the one the
    // new file causes; the socket's read timeout bounds the wait.
    write(&dir, "doc/product/section_02.md", "# Two\n");
    loop {
        let tree: Value = serde_json::from_str(&read_until(&mut socket, "tree_changed")).unwrap();
        if all_paths(&tree)
            .iter()
            .any(|p| p == "doc/product/section_02.md")
        {
            break;
        }
    }
    server.stop().unwrap();
}
