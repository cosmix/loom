//! Bridge tests: `WebFetch` requests the standing rules answer, the holder's session grant and
//! its revoke, and the rules' precedence.

use serde_json::json;

use super::fake_host::*;
use super::tests::wait_for;
use super::{BridgeError, EndCause, EndMode};
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{
    ChatDelta, DenyReason, FetchRule, FetchView, Item, PermissionStatus, SessionState,
};
use crate::plan_session::permission::Behavior;
use crate::plan_session::web_fetch::{FetchMode, FetchPolicy, FetchPolicyCell, TURNED_OFF};

fn harness() -> (Harness, FetchPolicyCell) {
    let cell = FetchPolicyCell::default();
    let shared = cell.clone();
    (Harness::ready(move |c| c.fetch_policy = shared), cell)
}

fn set(cell: &FetchPolicyCell, mode: FetchMode, domains: &[&str]) {
    let domains: Vec<String> = domains.iter().map(|d| (*d).to_owned()).collect();
    cell.set(FetchPolicy::validated(mode, &domains).unwrap());
}

fn fetch(h: &Harness, s: &Sess, url: &str) -> String {
    h.permission(s, "WebFetch", json!({"url": url, "prompt": "read it"}))
}

/// The card's status and fetch view.
fn card(h: &Harness, request: &str) -> Option<(PermissionStatus, Option<FetchView>)> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let mut items = turns.into_iter().flat_map(|turn| turn.items);
    items.find_map(|item| match item {
        Item::Permission {
            request_id,
            status,
            fetch,
            ..
        } if request_id == request => Some((status, fetch)),
        _ => None,
    })
}

fn wait_pending(h: &Harness, request: &str) {
    wait_until("the pending card", || {
        card(h, request).is_some_and(|(status, _)| status == PermissionStatus::Pending)
    });
}

/// The request was answered allow by `rule`, and no browser ever saw it pending.
fn assert_allowed_by(h: &Harness, s: &Sess, request: &str, rule: FetchRule) {
    let answer = wait_for("the allow answer", || h.answer(s, request));
    assert_eq!((answer.behavior, answer.reason), (Behavior::Allow, None));
    let (status, fetch) = card(h, request).expect("a transcript line");
    assert_eq!(status, PermissionStatus::Allowed);
    assert_eq!(fetch.and_then(|fetch| fetch.rule), Some(rule));
    assert!(!seen_pending(h, request), "a card asked for {request}");
}

fn seen_pending(h: &Harness, request: &str) -> bool {
    let deltas = h.deltas.lock().unwrap();
    deltas.iter().any(|delta| match delta {
        ChatDelta::ItemAdded { item, .. } | ChatDelta::ItemReplaced { item, .. } => matches!(
            item,
            Item::Permission { request_id, status: PermissionStatus::Pending, .. }
                if request_id == request
        ),
        _ => false,
    })
}

fn granted(h: &Harness) -> bool {
    let session = h.bridge.chat_snapshot(false).session;
    session.is_some_and(|view| view.fetch_grant)
}

#[test]
fn a_session_grant_allows_later_fetches_and_open_ones_but_no_other_tool() {
    let (h, _cell) = harness();
    let s = h.idle(Authoring);
    h.working(&s);
    let first = fetch(&h, &s, "https://docs.rs/serde");
    let second = fetch(&h, &s, "https://example.com/a");
    let bash = h.permission(&s, "Bash", json!({"command": "ls"}));
    wait_pending(&h, &bash);
    let refused = h.bridge.grant_fetches(&s.id, &bash);
    assert!(
        matches!(refused, Err(BridgeError::Refused(_))),
        "{refused:?}"
    );
    assert!(!granted(&h), "a Bash card granted fetches");
    h.bridge.grant_fetches(&s.id, &first).unwrap();
    assert!(granted(&h));
    for request in [&first, &second] {
        let answer = wait_for("the allow answer", || h.answer(&s, request));
        assert_eq!(answer.behavior, Behavior::Allow);
        let rule = card(&h, request).and_then(|(_, fetch)| fetch?.rule);
        assert_eq!(rule, Some(FetchRule::Session));
    }
    assert_eq!(
        card(&h, &bash).map(|(status, _)| status),
        Some(PermissionStatus::Pending)
    );
    assert!(h.answer(&s, &bash).is_none(), "the Bash card was answered");
    let later = fetch(&h, &s, "https://example.org/");
    assert_allowed_by(&h, &s, &later, FetchRule::Session);
    let shown = card(&h, &later).and_then(|(_, fetch)| fetch).unwrap();
    assert_eq!(shown.url, "https://example.org/");
    let other = h.permission(&s, "Bash", json!({"command": "pwd"}));
    wait_pending(&h, &other);
}

#[test]
fn ask_again_returns_to_a_card() {
    let (h, _cell) = harness();
    let s = h.idle(Authoring);
    h.working(&s);
    let first = fetch(&h, &s, "https://docs.rs/");
    wait_pending(&h, &first);
    h.bridge.grant_fetches(&s.id, &first).unwrap();
    h.bridge.revoke_fetches(&s.id).unwrap();
    assert!(!granted(&h));
    let next = fetch(&h, &s, "https://docs.rs/again");
    wait_pending(&h, &next);
    h.wait_state(&s, SessionState::Permission);
    assert!(h.answer(&s, &next).is_none());
}

#[test]
fn a_new_session_asks_again_after_a_grant() {
    let (h, _cell) = harness();
    let s = h.idle(Authoring);
    h.working(&s);
    let first = fetch(&h, &s, "https://docs.rs/");
    wait_pending(&h, &first);
    h.bridge.grant_fetches(&s.id, &first).unwrap();
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the end", || h.ended(&s.id).is_some());
    let next = h.idle(Authoring);
    assert!(!granted(&h), "the grant reached another session");
    h.working(&next);
    let request = fetch(&h, &next, "https://docs.rs/");
    wait_pending(&h, &request);
    assert!(h.answer(&next, &request).is_none());
}

#[test]
fn never_denies_every_fetch_even_after_a_grant() {
    let (h, cell) = harness();
    let s = h.idle(Authoring);
    h.working(&s);
    let first = fetch(&h, &s, "https://docs.rs/");
    wait_pending(&h, &first);
    h.bridge.grant_fetches(&s.id, &first).unwrap();
    set(&cell, FetchMode::Never, &["docs.rs"]);
    let request = fetch(&h, &s, "https://docs.rs/x");
    h.wait_deny(&s, &request, DenyReason::Settings);
    let answer = h.answer(&s, &request).unwrap();
    assert_eq!(answer.message.as_deref(), Some(TURNED_OFF));
    let (status, fetch) = card(&h, &request).unwrap();
    assert_eq!(status, PermissionStatus::Denied);
    assert_eq!(fetch.and_then(|fetch| fetch.rule), Some(FetchRule::Never));
    assert!(!seen_pending(&h, &request));
}

#[test]
fn a_settings_change_applies_from_the_running_sessions_next_fetch() {
    let (h, cell) = harness();
    let s = h.idle(Authoring);
    h.working(&s);
    let asked = fetch(&h, &s, "https://example.com/");
    wait_pending(&h, &asked);
    set(&cell, FetchMode::Always, &[]);
    let next = fetch(&h, &s, "https://example.com/next");
    assert_allowed_by(&h, &s, &next, FetchRule::Always);
    assert_eq!(
        card(&h, &asked).map(|(status, _)| status),
        Some(PermissionStatus::Pending)
    );
    let bash = h.permission(&s, "Bash", json!({"command": "ls"}));
    wait_pending(&h, &bash);
}

#[test]
fn a_listed_host_and_its_subdomains_are_allowed_and_a_look_alike_asks() {
    let (h, cell) = harness();
    set(&cell, FetchMode::Listed, &["docs.rs"]);
    let s = h.idle(Authoring);
    h.working(&s);
    let sub = fetch(&h, &s, "https://api.docs.rs/v1");
    assert_allowed_by(&h, &s, &sub, FetchRule::Listed);
    let evil = fetch(&h, &s, "https://evildocs.rs/");
    wait_pending(&h, &evil);
    assert!(h.answer(&s, &evil).is_none());
    assert_eq!(h.bridge.fetch_host(&s.id, &evil).unwrap(), "evildocs.rs");
    set(&cell, FetchMode::Listed, &["docs.rs", "evildocs.rs"]);
    h.bridge.answer_fetches(&s.id).unwrap();
    assert_eq!(h.answer(&s, &evil).unwrap().behavior, Behavior::Allow);
    let ip = fetch(&h, &s, "https://127.0.0.1/");
    wait_pending(&h, &ip);
    let refused = h.bridge.fetch_host(&s.id, &ip);
    assert!(
        matches!(refused, Err(BridgeError::Refused(_))),
        "{refused:?}"
    );
}

/// The card ended Denied and no rule is credited with it.
fn assert_denied_without_a_rule(h: &Harness, request: &str) {
    let (status, fetch) = card(h, request).expect("a transcript line");
    assert_eq!(status, PermissionStatus::Denied);
    assert_eq!(fetch.and_then(|fetch| fetch.rule), None);
}

#[test]
fn an_oversize_fetch_is_denied_as_too_large_even_under_always() {
    let (h, cell) = harness();
    set(&cell, FetchMode::Always, &[]);
    let s = h.idle(Authoring);
    h.working(&s);
    // Every string is under the spool's cut; only the whole input is over a card.
    let parts: Vec<String> = (0..20)
        .map(|i| format!("{i}{}", "b".repeat(4000)))
        .collect();
    let input = json!({"url": "https://docs.rs/", "prompt": "p", "extra": parts});
    let request = h.permission(&s, "WebFetch", input);
    h.wait_deny(&s, &request, DenyReason::HookError);
    let answer = h.answer(&s, &request).unwrap();
    let message = answer.message.as_deref().unwrap_or_default();
    assert!(message.contains("too large"), "{answer:?}");
    assert_denied_without_a_rule(&h, &request);
}

#[test]
fn a_session_ending_for_the_holders_absence_denies_a_new_fetch_even_under_always() {
    let (h, cell) = harness();
    set(&cell, FetchMode::Always, &[]);
    let s = h.idle(Authoring);
    h.working(&s);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::GraceExpired)
        .unwrap();
    let request = fetch(&h, &s, "https://docs.rs/");
    h.wait_deny(&s, &request, DenyReason::Holder);
    assert_denied_without_a_rule(&h, &request);
}
