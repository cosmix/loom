//! Tests for presence and display names.

use std::time::{Duration, SystemTime};

use crate::plan_server::authoring::{Authoring, Take};
use crate::plan_server::ids::{BrowserId, ConnCtx, ConnId};
use crate::plan_server::presence::{sanitize_name, Presence};
use crate::plan_server::wire::presence::PresenceEntry;

fn open(presence: &Presence, browser: &BrowserId, conn: u64) -> ConnCtx {
    let ctx = ConnCtx {
        id: ConnId(conn),
        browser: browser.clone(),
        public_id: presence.public_id(browser),
    };
    presence.opened(&ctx);
    ctx
}

fn new_browser() -> BrowserId {
    BrowserId::mint().unwrap()
}

#[test]
fn sanitize_drops_bidi_and_control_characters() {
    assert_eq!(sanitize_name("\u{202E}Dana\u{0007}"), "Dana");
    assert_eq!(
        sanitize_name("\u{200B}D\u{2067}a\u{FEFF}n\u{E0041}a\u{061C}\u{200F}"),
        "Dana"
    );
    assert_eq!(sanitize_name("  Ada Lovelace \n"), "Ada Lovelace");
}

#[test]
fn sanitize_keeps_32_chars_of_multi_byte_names() {
    let long = "\u{00E9}".repeat(40);
    assert_eq!(sanitize_name(&long), "\u{00E9}".repeat(32));
    let mixed = format!("{}x", "\u{1F600}".repeat(31));
    assert_eq!(sanitize_name(&format!("{mixed}tail")), mixed);
}

#[test]
fn sanitize_trims_space_left_at_the_cut() {
    let raw = format!("{} rest", "a".repeat(31));
    assert_eq!(sanitize_name(&raw), "a".repeat(31));
}

#[test]
fn all_control_name_shows_as_viewer_n() {
    let presence = Presence::new();
    let (a, b) = (new_browser(), new_browser());
    presence.public_id(&a);
    presence.set_name(&b, "\u{202E}\u{0007}\u{2066}\u{200E}");
    assert_eq!(sanitize_name("\u{202E}\u{0007}\u{2066}\u{200E}"), "");
    assert_eq!(presence.name_of(&a), "Viewer 1");
    assert_eq!(presence.name_of(&b), "Viewer 2");
}

#[test]
fn public_id_is_stable_per_browser() {
    let presence = Presence::new();
    let (a, b) = (new_browser(), new_browser());
    let first = presence.public_id(&a);
    assert_eq!(presence.public_id(&a), first);
    assert_ne!(presence.public_id(&b), first);
}

#[test]
fn name_of_does_not_record_unseen_browsers() {
    let presence = Presence::new();
    let (a, b) = (new_browser(), new_browser());
    assert_eq!(presence.name_of(&a), "", "an unseen browser has no name");
    presence.public_id(&b);
    // `a` was only looked up, so `b` is the first browser recorded.
    assert_eq!(presence.name_of(&b), "Viewer 1");
    assert_eq!(presence.name_of(&a), "");
}

#[test]
fn duplicate_name_shows_the_later_browser_as_viewer_n() {
    let presence = Presence::new();
    let (a, b) = (new_browser(), new_browser());
    let tab_b = open(&presence, &b, 1);
    let tab_a = open(&presence, &a, 2);
    presence.set_name(&a, "Dana");
    presence.set_name(&b, " Dana\u{202E}");
    assert_eq!(presence.name_of(&a), "Dana");
    assert_eq!(presence.name_of(&b), "Viewer 1");
    presence.set_name(&a, "Dana");
    assert_eq!(presence.name_of(&a), "Dana", "same name keeps its place");
    presence.closed(tab_a.id);
    assert_eq!(
        presence.name_of(&b),
        "Dana",
        "a disconnected browser holds no name"
    );
    presence.closed(tab_b.id);
}

#[test]
fn reading_must_be_a_doc_path() {
    let presence = Presence::new();
    let a = new_browser();
    let tab = open(&presence, &a, 7);
    let auth = Authoring::new(false);
    let reading = |path: &str| {
        presence.set_reading(tab.id, Some(path.to_owned()));
        presence.list(&auth).entries[0].reading.clone()
    };
    assert_eq!(
        reading("doc/plans/PLAN-x.md").as_deref(),
        Some("doc/plans/PLAN-x.md")
    );
    for bad in [
        "plans/PLAN-x.md",
        "doc/",
        "doc/../etc/passwd",
        "doc//x.md",
        "/doc/x.md",
        "doc/a\\b.md",
        "doc/a\u{0007}.md",
        "src/main.rs",
    ] {
        assert_eq!(reading(bad), None, "{bad:?} must not be stored");
    }
    presence.set_reading(tab.id, None);
    assert_eq!(presence.list(&auth).entries[0].reading, None);
}

#[test]
fn list_shows_open_browsers_and_the_away_holder() {
    let presence = Presence::new();
    let auth = Authoring::new(false);
    let (a, b, c) = (new_browser(), new_browser(), new_browser());
    let tab_a = open(&presence, &a, 1);
    let tab_b1 = open(&presence, &b, 2);
    let tab_b2 = open(&presence, &b, 3);
    let tab_c = open(&presence, &c, 4);
    presence.set_name(&b, "Bo");
    presence.set_reading(tab_b1.id, Some("doc/one.md".into()));
    presence.set_reading(tab_b2.id, Some("doc/two.md".into()));
    auth.require(&a, &tab_a.public_id, Take::TakeIfFree, false)
        .unwrap();
    auth.connection_opened(&a);
    presence.closed(tab_a.id);
    presence.closed(tab_c.id);
    auth.connection_closed(&a, SystemTime::UNIX_EPOCH + Duration::from_secs(60));
    let entries = presence.list(&auth).entries;
    let expected = vec![
        PresenceEntry {
            public_id: tab_a.public_id.clone(),
            name: "Viewer 1".into(),
            holder: true,
            away: true,
            reading: None,
            tabs: 0,
        },
        PresenceEntry {
            public_id: tab_b1.public_id.clone(),
            name: "Bo".into(),
            holder: false,
            away: false,
            reading: Some("doc/two.md".into()),
            tabs: 2,
        },
    ];
    assert_eq!(entries, expected);
}

#[test]
fn closing_an_unknown_connection_changes_nothing() {
    let presence = Presence::new();
    let a = new_browser();
    open(&presence, &a, 1);
    presence.closed(ConnId(99));
    presence.set_reading(ConnId(99), Some("doc/x.md".into()));
    let entries = presence.list(&Authoring::new(false)).entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].tabs, 1);
    assert_eq!(entries[0].reading, None);
}
