//! Web fetch settings through the planning server's loopback HTTP helpers: who may change
//! them, what is refused, what is saved and what the bridge's cell then holds.

use serde_json::{json, Value};

use super::{routes, settings, WebFetchSettings};
use crate::plan_server::lifecycle::safe;
use crate::plan_server::tests::support::{
    bootstrap, get, json_body, options, post_json, project, skip, status, test_routes,
};
use crate::plan_server::{start_with_routes, RunningServer};
use crate::plan_session::web_fetch::{FetchMode, FetchPolicy};

const PATH: &str = "/api/web-fetch";

fn serve(project: &tempfile::TempDir) -> RunningServer {
    let mut table = routes();
    table.extend(test_routes());
    start_with_routes(options(project), table).expect("start the planning server")
}

fn take(server: &RunningServer, cookies: &str) {
    let response = post_json(server, "/api/test/take", cookies, "");
    assert_eq!(status(&response), 200, "{}", safe(&response));
}

fn read(server: &RunningServer, cookies: &str) -> Value {
    let response = get(server, PATH, cookies);
    assert_eq!(status(&response), 200, "{}", safe(&response));
    json_body(&response)
}

fn refused(response: &str, expected: u16, code: &str) {
    assert_eq!(status(response), expected, "{}", safe(response));
    assert_eq!(json_body(response)["error"], code, "{}", safe(response));
}

#[test]
fn only_the_holder_changes_the_policy_and_every_browser_reads_it() {
    if skip("only_the_holder_changes_the_policy_and_every_browser_reads_it") {
        return;
    }
    let dir = project();
    let server = serve(&dir);
    let holder = bootstrap(&server);
    let observer = bootstrap(&server);
    assert_eq!(
        read(&server, &observer),
        json!({"mode": "ask", "domains": [], "revision": 0})
    );
    let body = json!({"mode": "always", "domains": [], "revision": 0}).to_string();
    refused(&post_json(&server, PATH, &holder, &body), 403, "no_holder");
    take(&server, &holder);
    refused(
        &post_json(&server, PATH, &observer, &body),
        403,
        "not_holder",
    );
    assert_eq!(settings(&server.ctx).current(), FetchPolicy::default());
    let body = json!({
        "mode": "listed", "domains": ["Docs.RS", "bücher.de", "docs.rs"], "revision": 0
    });
    let response = post_json(&server, PATH, &holder, &body.to_string());
    assert_eq!(status(&response), 200, "{}", safe(&response));
    let expected =
        json!({"mode": "listed", "domains": ["docs.rs", "xn--bcher-kva.de"], "revision": 1});
    assert_eq!(json_body(&response), expected);
    assert_eq!(read(&server, &observer), expected);
    let cell = super::policy(&server.ctx).get();
    assert_eq!(
        cell.mode,
        FetchMode::Listed,
        "the bridge's cell missed the change"
    );
}

#[test]
fn an_invalid_entry_or_field_changes_nothing() {
    if skip("an_invalid_entry_or_field_changes_nothing") {
        return;
    }
    let dir = project();
    let server = serve(&dir);
    let holder = bootstrap(&server);
    take(&server, &holder);
    let saved = json!({"mode": "listed", "domains": ["docs.rs"], "revision": 0}).to_string();
    assert_eq!(status(&post_json(&server, PATH, &holder, &saved)), 200);
    for entry in [
        "https://evil.example",
        "*.example.com",
        "10.0.0.1",
        "example.com:8080",
    ] {
        let body =
            json!({"mode": "always", "domains": ["docs.rs", entry], "revision": 1}).to_string();
        let response = post_json(&server, PATH, &holder, &body);
        refused(&response, 400, "invalid_domain");
        let message = json_body(&response)["message"].as_str().unwrap().to_owned();
        assert!(message.contains(entry), "{message}");
    }
    let unknown =
        json!({"mode": "always", "domains": [], "revision": 1, "grant": true}).to_string();
    refused(
        &post_json(&server, PATH, &holder, &unknown),
        400,
        "bad_request",
    );
    let mode = json!({"mode": "sometimes", "domains": [], "revision": 1}).to_string();
    refused(
        &post_json(&server, PATH, &holder, &mode),
        400,
        "bad_request",
    );
    assert_eq!(
        read(&server, &holder),
        json!({"mode": "listed", "domains": ["docs.rs"], "revision": 1})
    );
}

#[test]
fn the_saved_policy_survives_a_restart_and_a_bad_file_asks() {
    let dir = project();
    let runtime = crate::planning::runtime::runtime_paths(dir.path()).unwrap();
    let saved = WebFetchSettings::load(runtime.clone());
    let policy = FetchPolicy::validated(FetchMode::Never, &["docs.rs".to_owned()]).unwrap();
    saved.replace(policy.clone(), 0).unwrap();
    assert_eq!(WebFetchSettings::load(runtime.clone()).current(), policy);
    saved.allow_host("api.example.com").unwrap();
    let grown = WebFetchSettings::load(runtime.clone()).current();
    assert_eq!(grown.mode, FetchMode::Never, "listing a host changed Never");
    assert_eq!(grown.domains, ["docs.rs", "api.example.com"]);
    let file = runtime.dir.join("web-fetch.json");
    let planted = json!({"mode": "always", "domains": ["*.evil.example"]}).to_string();
    std::fs::write(&file, planted).unwrap();
    assert_eq!(
        WebFetchSettings::load(runtime).current(),
        FetchPolicy::default()
    );
}

#[test]
fn a_change_built_on_an_older_read_is_refused_with_409() {
    if skip("a_change_built_on_an_older_read_is_refused_with_409") {
        return;
    }
    let dir = project();
    let server = serve(&dir);
    let holder = bootstrap(&server);
    take(&server, &holder);
    let first = json!({"mode": "listed", "domains": ["docs.rs"], "revision": 0}).to_string();
    assert_eq!(status(&post_json(&server, PATH, &holder, &first)), 200);
    // A second tab still holds revision 0, and "Always allow" moves the policy on as well.
    settings(&server.ctx).allow_host("api.example.com").unwrap();
    let stale = json!({"mode": "never", "domains": [], "revision": 1}).to_string();
    refused(
        &post_json(&server, PATH, &holder, &stale),
        409,
        "stale_policy",
    );
    let now = read(&server, &holder);
    assert_eq!(now["domains"], json!(["docs.rs", "api.example.com"]));
    assert_eq!(now["revision"], 2);
    let fresh = json!({"mode": "never", "domains": [], "revision": 2}).to_string();
    assert_eq!(status(&post_json(&server, PATH, &holder, &fresh)), 200);
    assert_eq!(settings(&server.ctx).current().mode, FetchMode::Never);
}

#[test]
fn the_largest_valid_policy_still_fits_the_post_body_cap() {
    if skip("the_largest_valid_policy_still_fits_the_post_body_cap") {
        return;
    }
    let dir = project();
    let server = serve(&dir);
    let holder = bootstrap(&server);
    take(&server, &holder);
    let long = |n: usize| format!("{n:03}{}.example.com", "a".repeat(55));
    let mut domains: Vec<String> = Vec::new();
    while FetchPolicy::validated(
        FetchMode::Never,
        &[domains.clone(), vec![long(domains.len())]].concat(),
    )
    .is_ok()
    {
        domains.push(long(domains.len()));
    }
    assert!(domains.len() > 30, "{} entries fit", domains.len());
    let body = json!({"mode": "never", "domains": domains, "revision": 0}).to_string();
    assert!(body.len() < crate::web_server::http::MAX_BODY_BYTES);
    let response = post_json(&server, PATH, &holder, &body);
    assert_eq!(status(&response), 200, "{}", safe(&response));
}

#[test]
fn a_symlinked_policy_file_is_refused_and_asks() {
    use std::os::unix::fs::symlink;
    let dir = project();
    let runtime = crate::planning::runtime::runtime_paths(dir.path()).unwrap();
    let saved = WebFetchSettings::load(runtime.clone());
    saved.replace(FetchPolicy::default(), 0).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("elsewhere.json");
    std::fs::write(
        &target,
        json!({"mode": "always", "domains": []}).to_string(),
    )
    .unwrap();
    let file = runtime.dir.join("web-fetch.json");
    std::fs::remove_file(&file).unwrap();
    symlink(&target, &file).unwrap();
    assert!(super::store::read(&runtime).is_err(), "the link was read");
    assert_eq!(
        WebFetchSettings::load(runtime).current(),
        FetchPolicy::default()
    );
}

#[test]
fn a_symlink_at_the_temp_path_does_not_redirect_the_write() {
    use std::os::unix::fs::symlink;
    let dir = project();
    let runtime = crate::planning::runtime::runtime_paths(dir.path()).unwrap();
    let saved = WebFetchSettings::load(runtime.clone());
    saved.replace(FetchPolicy::default(), 0).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let victim = outside.path().join("victim.txt");
    std::fs::write(&victim, "untouched").unwrap();
    symlink(&victim, runtime.dir.join(".web-fetch.json.tmp")).unwrap();
    let policy = FetchPolicy::validated(FetchMode::Never, &[]).unwrap();
    saved.replace(policy.clone(), 1).unwrap();
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "untouched");
    let file = runtime.dir.join("web-fetch.json");
    assert!(!std::fs::symlink_metadata(&file).unwrap().is_symlink());
    assert_eq!(WebFetchSettings::load(runtime).current(), policy);
}
