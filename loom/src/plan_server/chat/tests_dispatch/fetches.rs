//! The fetch commands from a card: only the holder may grow the rules, and "Always allow"
//! lists the host the server reads from the request.

use serde_json::json;

use super::super::tests::{browser, ok_result, wait_for, Fixture, Tab};
use super::{fixture, refused};
use crate::plan_server::authoring::Refusal;
use crate::plan_server::web_fetch::settings;
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::model::{FetchRule, Item, PermissionStatus, SessionState};
use crate::plan_session::web_fetch::{FetchMode, FetchPolicy};

fn request(f: &Fixture, s: &LaunchSpec, tool: &str, input: serde_json::Value) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let extra = json!({"request_id": id, "tool_name": tool, "tool_input": input});
    f.spool(s, "PermissionRequest", extra);
    id
}

fn status_of(f: &Fixture, request_id: &str) -> Option<(PermissionStatus, Option<FetchRule>)> {
    let turns = f.bridge.chat_snapshot(true).turns;
    let mut items = turns.into_iter().flat_map(|turn| turn.items);
    items.find_map(|item| match item {
        Item::Permission {
            request_id: id,
            status,
            fetch,
            ..
        } if id == request_id => Some((status, fetch.and_then(|fetch| fetch.rule))),
        _ => None,
    })
}

fn wait_pending(f: &Fixture, request_id: &str) {
    wait_for("the card", || {
        let pending = status_of(f, request_id)?.0 == PermissionStatus::Pending;
        pending.then_some(())
    });
}

fn working(f: &Fixture, s: &LaunchSpec) {
    f.spool(s, "UserPromptSubmit", json!({"prompt": "look it up"}));
    f.wait_state(s, SessionState::Working);
}

fn client() -> Option<String> {
    Some("c1".into())
}

#[test]
fn an_observer_cannot_grant_list_or_revoke() {
    let f = fixture(false);
    let holder = f.connect(&browser());
    let observer = f.connect(&browser());
    let s = f.idle_chat(&holder);
    working(&f, &s);
    let fetch = request(
        &f,
        &s,
        "WebFetch",
        json!({"url": "https://docs.rs/", "prompt": "p"}),
    );
    wait_pending(&f, &fetch);
    let error = f.ctx.refusal_message(&Refusal::NotHolder {
        holder: holder.conn.public_id.clone(),
    });
    let commands = [
        ChatCommand::AllowSessionFetches {
            client_id: client(),
            request_id: fetch.clone(),
        },
        ChatCommand::AllowFetchHost {
            client_id: client(),
            request_id: fetch.clone(),
        },
        ChatCommand::AskFetchesAgain {
            client_id: client(),
        },
    ];
    for command in commands {
        assert_eq!(f.command(&observer, command), refused(&error));
    }
    assert_eq!(settings(&f.ctx).current(), FetchPolicy::default());
    let view = f.bridge.chat_snapshot(false).session.unwrap();
    assert!(!view.fetch_grant);
    assert_eq!(
        status_of(&f, &fetch),
        Some((PermissionStatus::Pending, None))
    );
}

/// Neither fetch command acts on a card that is not a `WebFetch`.
fn refuse_fetch_commands_on(f: &Fixture, holder: &Tab, request_id: &str) {
    let host = ChatCommand::AllowFetchHost {
        client_id: client(),
        request_id: request_id.to_owned(),
    };
    assert_eq!(f.command(holder, host)["ok"], false);
    let session = ChatCommand::AllowSessionFetches {
        client_id: client(),
        request_id: request_id.to_owned(),
    };
    assert_eq!(f.command(holder, session)["ok"], false);
}

#[test]
fn always_allow_lists_the_requests_host_and_allows_its_subdomains_next() {
    let f = fixture(false);
    let holder = f.connect(&browser());
    let s = f.idle_chat(&holder);
    working(&f, &s);
    let url = "https://User@API.Docs.RS:8443/crate?q=1";
    let fetch = request(&f, &s, "WebFetch", json!({"url": url, "prompt": "p"}));
    wait_pending(&f, &fetch);
    let bash = request(&f, &s, "Bash", json!({"command": "ls"}));
    wait_pending(&f, &bash);
    refuse_fetch_commands_on(&f, &holder, &bash);
    let command = ChatCommand::AllowFetchHost {
        client_id: client(),
        request_id: fetch.clone(),
    };
    assert_eq!(f.command(&holder, command), ok_result());
    let policy = settings(&f.ctx).current();
    assert_eq!(policy.mode, FetchMode::Listed);
    assert_eq!(policy.domains, ["api.docs.rs"]);
    let allowed = Some((PermissionStatus::Allowed, Some(FetchRule::Listed)));
    assert_eq!(status_of(&f, &fetch), allowed);
    assert_eq!(
        status_of(&f, &bash),
        Some((PermissionStatus::Pending, None))
    );
    let next = request(
        &f,
        &s,
        "WebFetch",
        json!({"url": "https://v2.api.docs.rs/", "prompt": "p"}),
    );
    wait_for("the listed fetch", || {
        (status_of(&f, &next) == allowed).then_some(())
    });
    let other = request(
        &f,
        &s,
        "WebFetch",
        json!({"url": "https://docs.rs/", "prompt": "p"}),
    );
    wait_pending(&f, &other);
}

#[test]
fn always_allow_is_refused_for_a_host_the_list_cannot_hold() {
    let f = fixture(false);
    let holder = f.connect(&browser());
    let s = f.idle_chat(&holder);
    working(&f, &s);
    for url in [
        "http://localhost:8080/",
        "http://intranet/",
        "http://app.localhost/",
    ] {
        let fetch = request(&f, &s, "WebFetch", json!({"url": url, "prompt": "p"}));
        wait_pending(&f, &fetch);
        let host = ChatCommand::AllowFetchHost {
            client_id: client(),
            request_id: fetch.clone(),
        };
        assert_eq!(f.command(&holder, host)["ok"], false, "{url}");
        assert_eq!(settings(&f.ctx).current(), FetchPolicy::default(), "{url}");
        assert_eq!(
            status_of(&f, &fetch),
            Some((PermissionStatus::Pending, None))
        );
    }
}
