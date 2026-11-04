//! Contract `drafts-survive-a-restart`: kickoff drafts stored under the runtime `kickoffs`
//! directory are listed by a freshly started server, newest first, with the user's rename
//! winning over the agent's name and no draft reported live without a running session.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use tempfile::TempDir;

use super::support::{bootstrap, get, json_body, options, project, serve, skip, status};
use crate::planning::runtime::{create_runtime_dir, server_runtime_paths};

const FIRST: &str = "11111111-1111-4111-8111-111111111111";
const SECOND: &str = "22222222-2222-4222-8222-222222222222";

/// A stored draft as the draft store writes it, flagged live as it was before the restart.
fn draft(session: &str, updated_at: &str, name: &str, name_override: Option<&str>) -> Value {
    json!({
        "session": session,
        "started_at": "2026-11-04T10:47:11Z",
        "updated_at": updated_at,
        "messages": ["i would like to add support for microvm test environment"],
        "shape": {
            "size": "feature",
            "name": name,
            "problem": "Tests need isolation.",
            "decisions": [],
            "questions": ["Which hosts?"],
            "proposal": false,
            "sections": []
        },
        "name_override": name_override,
        "live": true
    })
}

fn write_draft(dir: &Path, value: &Value) {
    let session = value["session"].as_str().expect("a session id");
    let text = serde_json::to_string_pretty(value).expect("serialize a draft");
    fs::write(dir.join(format!("{session}.json")), text).expect("write a draft file");
}

/// Write both drafts where the server reads them; returns the runtime directory to remove.
fn store_drafts(project: &TempDir) -> PathBuf {
    let (runtime, _) = server_runtime_paths(&options(project).root).expect("runtime paths");
    create_runtime_dir(&runtime).expect("create the runtime directory");
    let kickoffs = runtime.dir.join("kickoffs");
    fs::create_dir_all(&kickoffs).expect("create the kickoffs directory");
    let named = draft(FIRST, "2026-11-04T10:50:00Z", "microvm-test-env", None);
    write_draft(&kickoffs, &named);
    let renamed = draft(
        SECOND,
        "2026-11-04T09:00:00Z",
        "x-name",
        Some("renamed-work"),
    );
    write_draft(&kickoffs, &renamed);
    runtime.dir
}

#[test]
fn a_stored_kickoff_draft_is_listed_after_a_restart() {
    if skip("a_stored_kickoff_draft_is_listed_after_a_restart") {
        return;
    }
    let project = project();
    let runtime_dir = store_drafts(&project);

    let server = serve(&project);
    let cookies = bootstrap(&server);
    let response = get(&server, "/api/kickoffs", &cookies);
    server.stop().expect("stop");
    let _ = fs::remove_dir_all(&runtime_dir);

    assert_eq!(status(&response), 200, "{response}");
    let drafts = json_body(&response)["drafts"].clone();
    let listed = drafts
        .as_array()
        .unwrap_or_else(|| panic!("a drafts array: {response}"));
    assert_eq!(listed.len(), 2, "{drafts}");
    let first = &listed[0];
    assert_eq!(first["session"], FIRST, "newest updated_at first: {drafts}");
    assert_eq!(first["name"], "microvm-test-env", "{drafts}");
    assert_eq!(first["size"], "feature", "{drafts}");
    assert_eq!(first["problem"], "Tests need isolation.", "{drafts}");
    assert_eq!(
        first["live"], false,
        "no session runs after a restart: {drafts}"
    );
    let second = &listed[1];
    assert_eq!(second["session"], SECOND, "{drafts}");
    assert_eq!(
        second["name"], "renamed-work",
        "the rename beats the agent: {drafts}"
    );
    assert_eq!(second["live"], false, "{drafts}");
}
