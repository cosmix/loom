//! `GET /api/kickoffs/<session>`: one stored draft with its shape and the user's messages, the
//! rename applied as the shape's name, and a 404 for an id that names no draft.

use std::fs;

use serde_json::json;

use super::support::{bootstrap, get, json_body, options, project, serve, skip, status};
use crate::planning::runtime::{create_runtime_dir, server_runtime_paths};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

/// The stored file of [`SESSION`]: renamed, with a shape, and flagged live as a crash leaves it.
fn draft_json() -> serde_json::Value {
    json!({
        "session": SESSION,
        "started_at": "2026-11-04T10:47:11Z",
        "updated_at": "2026-11-04T10:50:00Z",
        "messages": ["first idea", "second idea"],
        "shape": {"size": "plan", "name": "agent-name", "proposal": false,
            "problem": "A problem", "decisions": ["Use one VM."],
            "questions": [], "sections": []},
        "name_override": "my-plan",
        "live": true
    })
}

fn assert_detail(found: &str) {
    assert_eq!(status(found), 200, "{found}");
    let detail = json_body(found);
    assert_eq!(detail["draft"]["session"], SESSION);
    assert_eq!(detail["draft"]["name"], "my-plan");
    assert_eq!(detail["draft"]["size"], "plan");
    assert_eq!(
        detail["draft"]["live"], false,
        "no session runs after a restart"
    );
    assert_eq!(
        detail["shape"]["name"], "my-plan",
        "the rename is the shape's name"
    );
    assert_eq!(detail["shape"]["decisions"], json!(["Use one VM."]));
    assert_eq!(detail["messages"], json!(["first idea", "second idea"]));
}

#[test]
fn a_draft_is_fetched_with_its_shape_and_messages() {
    if skip("a_draft_is_fetched_with_its_shape_and_messages") {
        return;
    }
    let project = project();
    let (runtime, _) = server_runtime_paths(&options(&project).root).expect("runtime paths");
    create_runtime_dir(&runtime).expect("create the runtime directory");
    let kickoffs = runtime.dir.join("kickoffs");
    fs::create_dir_all(&kickoffs).expect("create the kickoffs directory");
    let draft = draft_json().to_string();
    fs::write(kickoffs.join(format!("{SESSION}.json")), draft).expect("write a draft");

    let server = serve(&project);
    let cookies = bootstrap(&server);
    let found = get(&server, &format!("/api/kickoffs/{SESSION}"), &cookies);
    let malformed = get(&server, "/api/kickoffs/not-a-session", &cookies);
    let unknown = get(
        &server,
        "/api/kickoffs/22222222-2222-4222-8222-222222222222",
        &cookies,
    );
    server.stop().expect("stop");
    let _ = fs::remove_dir_all(&runtime.dir);

    assert_detail(&found);
    assert_eq!(status(&malformed), 404, "{malformed}");
    assert_eq!(status(&unknown), 404, "{unknown}");
}
