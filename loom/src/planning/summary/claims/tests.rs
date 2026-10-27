//! Claims: exclusive create, session-scoped release, unparsable files, session clearing and
//! a symlinked claims directory.

use std::fs;
use std::os::unix::fs::symlink;

use super::*;
use crate::planning::summary::evaluate::fixtures::{fixture, Fixture, ID1};

const PLAN: &str = "plans/x";

fn claim(fx: &Fixture, id: &str, session: Option<&str>) -> Claim {
    create_claim_as(&fx.paths, id, session.map(str::to_string))
        .expect("create")
        .expect("unclaimed")
}

fn claimed_ids(fx: &Fixture) -> Vec<String> {
    list_claims(&fx.paths)
        .expect("list")
        .into_iter()
        .map(|c| c.summary)
        .collect()
}

#[test]
fn second_claim_is_refused_with_the_existing_one() {
    let fx = fixture(&[]);
    let first = claim(&fx, ID1, Some("s1"));
    assert_eq!(
        (first.summary.as_str(), first.session.as_deref()),
        (ID1, Some("s1"))
    );
    assert_eq!(first.pid, std::process::id());
    assert!(chrono::DateTime::parse_from_rfc3339(&first.created_at).is_ok());
    let file = fx.paths.claims.join("product__section_01.json");
    assert!(file.is_file(), "claim stored as product__section_01.json");
    let second = create_claim_as(&fx.paths, ID1, Some("s2".into())).expect("create");
    assert_eq!(second, Err(first.clone()), "the existing claim is returned");
    assert_eq!(list_claims(&fx.paths).expect("list"), vec![first]);
    assert!(
        create_claim_as(&fx.paths, "plans/../x", None).is_err(),
        "invalid id"
    );
}

#[test]
fn release_is_scoped_to_the_session() {
    let fx = fixture(&[]);
    claim(&fx, ID1, Some("s1"));
    claim(&fx, PLAN, Some("s2"));
    let ids = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let refused = release_as(&fx.paths, &ids(&[ID1, PLAN]), Some("s1"));
    let message = format!(
        "{:#}",
        refused.expect_err("released another session's claim")
    );
    assert!(
        message.contains("s2") && message.contains(PLAN),
        "{message}"
    );
    assert_eq!(
        claimed_ids(&fx),
        ids(&[PLAN, ID1]),
        "nothing deleted on refusal"
    );
    let released = release_as(&fx.paths, &ids(&[ID1, "product"]), Some("s1")).expect("own");
    assert_eq!(released, ids(&[ID1]), "an unclaimed id is skipped");
    let released = release_as(&fx.paths, &ids(&[PLAN]), None).expect("terminal");
    assert_eq!(released, ids(&[PLAN]));
    assert!(claimed_ids(&fx).is_empty());
}

#[test]
fn unparsable_claim_counts_with_no_session() {
    let fx = fixture(&[]);
    fs::create_dir_all(&fx.paths.claims).unwrap();
    fs::write(fx.paths.claims.join("plans__x.json"), "{not json").unwrap();
    let other = r#"{"summary":"product","session":"s9","pid":3,"created_at":"t"}"#;
    fs::write(fx.paths.claims.join("plans__y.json"), other).unwrap();
    let ctl = r#"{"summary":"plans/z","session":"s\u001b[31m1","pid":4,"created_at":"t"}"#;
    fs::write(fx.paths.claims.join("plans__z.json"), ctl).unwrap();
    fs::write(fx.paths.claims.join("notes.txt"), "ignored").unwrap();
    let claims = list_claims(&fx.paths).expect("list");
    let summary: Vec<(&str, Option<&str>)> = claims
        .iter()
        .map(|c| (c.summary.as_str(), c.session.as_deref()))
        .collect();
    let want = vec![
        ("plans/x", None),
        ("plans/y", None),
        ("plans/z", Some("s[31m1")),
    ];
    assert_eq!(summary, want);
    let existing = create_claim_as(&fx.paths, PLAN, Some("s1".into())).expect("create");
    assert_eq!(
        existing.map_err(|c| c.session),
        Err(None),
        "refused on the unparsable file"
    );
}

#[test]
fn clear_session_claims_removes_only_that_session() {
    let fx = fixture(&[]);
    claim(&fx, ID1, Some("s1"));
    claim(&fx, PLAN, Some("s2"));
    claim(&fx, "product", None);
    assert_eq!(
        clear_session_claims(&fx.paths, "s1").expect("clear"),
        vec![ID1.to_string()]
    );
    assert_eq!(
        claimed_ids(&fx),
        vec!["plans/x".to_string(), "product".to_string()]
    );
    assert!(clear_session_claims(&fx.paths, "s1")
        .expect("again")
        .is_empty());
}

#[test]
fn symlinked_claims_directory_is_refused() {
    let fx = fixture(&[]);
    fs::create_dir_all(&fx.paths.dir).unwrap();
    let elsewhere = fx.root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("plans__x.json"), CLAIM_JSON).unwrap();
    symlink(&elsewhere, &fx.paths.claims).unwrap();
    assert!(list_claims(&fx.paths).is_err(), "listed through the link");
    assert!(
        create_claim_as(&fx.paths, ID1, None).is_err(),
        "created through the link"
    );
    assert!(!elsewhere.join("product__section_01.json").exists());
    assert!(
        delete_claim(&fx.paths, PLAN).is_err(),
        "deleted through the link"
    );
    assert!(elsewhere.join("plans__x.json").exists());
}

const CLAIM_JSON: &str = r#"{"summary":"plans/x","session":"s1","pid":1,"created_at":"t"}"#;

#[test]
fn no_runtime_dir_means_no_claims() {
    let fx = fixture(&[]);
    assert!(list_claims(&fx.paths).expect("list").is_empty());
    delete_claim(&fx.paths, ID1).expect("absent claim");
    assert!(release_as(&fx.paths, &[ID1.to_string()], Some("s1"))
        .expect("none")
        .is_empty());
}

#[test]
fn session_text_is_sanitised_and_drafts_live_under_the_runtime_dir() {
    assert_eq!(sanitize_session("ab\u{7}c\n"), Some("abc".to_string()));
    assert_eq!(sanitize_session("\u{1b}"), None);
    assert_eq!(
        sanitize_session(&"x".repeat(300)).map(|s| s.len()),
        Some(128)
    );
    let fx = fixture(&[]);
    assert_eq!(
        draft_path(&fx.paths, ID1),
        fx.paths.drafts.join("product/section_01.md")
    );
    assert_eq!(draft_path(&fx.paths, "../x"), std::path::PathBuf::new());
    assert_eq!(
        id_from_file("plans__a__b.json").as_deref(),
        Some("plans/a__b")
    );
}
