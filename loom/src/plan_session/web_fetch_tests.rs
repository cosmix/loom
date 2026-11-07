//! The fetch rules: precedence, host matching and allowlist validation.

use serde_json::json;

use super::*;
use crate::web_server::http::MAX_BODY_BYTES;

fn policy(mode: FetchMode, domains: &[&str]) -> FetchPolicy {
    let domains: Vec<String> = domains.iter().map(|d| (*d).to_owned()).collect();
    FetchPolicy::validated(mode, &domains).unwrap()
}

fn fetch(url: &str) -> FetchView {
    fetch_view(&json!({"url": url, "prompt": "read it"}))
}

#[test]
fn precedence_is_never_always_listed_session_then_the_card() {
    let listed = fetch("https://docs.rs/serde");
    let other = fetch("https://example.com/");
    let cases = [
        (FetchMode::Never, &listed, true, Some(FetchRule::Never)),
        (FetchMode::Never, &other, false, Some(FetchRule::Never)),
        (FetchMode::Always, &other, false, Some(FetchRule::Always)),
        (FetchMode::Always, &other, true, Some(FetchRule::Always)),
        (FetchMode::Listed, &listed, true, Some(FetchRule::Listed)),
        (FetchMode::Listed, &other, true, Some(FetchRule::Session)),
        (FetchMode::Listed, &other, false, None),
        // The list waits for the Listed mode; Ask still honours the session's grant.
        (FetchMode::Ask, &listed, false, None),
        (FetchMode::Ask, &listed, true, Some(FetchRule::Session)),
    ];
    for (mode, view, granted, expected) in cases {
        let rule = policy(mode, &["docs.rs"]).rule(view, granted);
        assert_eq!(rule, expected, "{mode:?} {} granted={granted}", view.url);
    }
}

#[test]
fn a_listed_domain_covers_itself_and_its_subdomains_only() {
    let list = policy(FetchMode::Listed, &["docs.rs"]);
    for url in [
        "https://docs.rs/x",
        "https://api.docs.rs/",
        "http://a.b.docs.rs:8080/p?q",
        "https://DOCS.RS./",
        "https://docs.rs@docs.rs/",
    ] {
        assert!(list.lists(fetch(url).host.as_deref()), "{url} is listed");
    }
    for url in [
        "https://evildocs.rs/",
        "https://docs.rs.evil.com/",
        "https://docs.rs@evil.com/",
        "https://rs/",
        "ftp://docs.rs/",
        "not a url",
        "https://127.0.0.1/",
        "",
    ] {
        assert!(
            !list.lists(fetch(url).host.as_deref()),
            "{url} is not listed"
        );
    }
}

#[test]
fn hosts_compare_in_punycode() {
    let list = policy(FetchMode::Listed, &["bücher.de"]);
    assert_eq!(list.domains, ["xn--bcher-kva.de"]);
    assert!(list.lists(fetch("https://www.BÜCHER.de/").host.as_deref()));
    assert!(list.lists(fetch("https://xn--bcher-kva.de/").host.as_deref()));
}

#[test]
fn an_ip_address_host_is_never_listable() {
    for url in ["https://127.0.0.1/", "https://[::1]/", "https://0x7f.1/"] {
        assert_eq!(fetch(url).host, None, "{url}");
    }
}

#[test]
fn invalid_allowlist_entries_are_refused_with_a_reason() {
    let refused = [
        ("", "empty"),
        ("https://docs.rs", "bare host name"),
        ("docs.rs/path", "bare host name"),
        ("docs.rs:443", "bare host name"),
        ("*.docs.rs", "bare host name"),
        ("user@docs.rs", "bare host name"),
        ("docs rs", "bare host name"),
        ("docs%2ers", "bare host name"),
        ("[::1]", "bare host name"),
        ("127.0.0.1", "IP address"),
        ("0x7f.1", "IP address"),
        ("localhost", "two labels"),
        ("localhost.", "two labels"),
        ("app.localhost", "names this machine"),
        ("a.b.LOCALHOST", "names this machine"),
        ("docs..rs", "valid host name"),
        (".docs.rs", "valid host name"),
    ];
    for (entry, why) in refused {
        let error = validate_domain(entry).unwrap_err();
        assert!(error.contains(why), "{entry:?}: {error}");
    }
}

#[test]
fn valid_entries_are_normalised_and_deduplicated() {
    assert_eq!(validate_domain("  Docs.RS. ").unwrap(), "docs.rs");
    let entries = ["docs.rs".to_owned(), "DOCS.rs".to_owned()];
    let validated = FetchPolicy::validated(FetchMode::Ask, &entries).unwrap();
    assert_eq!(validated.domains, ["docs.rs"]);
    let many: Vec<String> = (0..=MAX_DOMAINS).map(|n| format!("h{n}.x")).collect();
    assert!(FetchPolicy::validated(FetchMode::Listed, &many).is_err());
}

#[test]
fn the_domain_count_limit_counts_distinct_entries_as_they_are_added() {
    let full: Vec<String> = (0..MAX_DOMAINS).map(|n| format!("h{n}.x")).collect();
    let repeated = [full.clone(), full.clone()].concat();
    let kept = FetchPolicy::validated(FetchMode::Listed, &repeated).unwrap();
    assert_eq!(kept.domains.len(), MAX_DOMAINS);
    let one_more = [full, vec!["extra.x".to_owned()]].concat();
    let error = FetchPolicy::validated(FetchMode::Listed, &one_more).unwrap_err();
    assert!(error.contains("at most"), "{error}");
}

#[test]
fn a_list_that_would_serialize_past_the_post_body_cap_is_refused() {
    let long = |n: usize| format!("{n:03}{}.example.com", "a".repeat(55));
    let mut domains: Vec<String> = Vec::new();
    loop {
        let mut next = domains.clone();
        next.push(long(domains.len()));
        match FetchPolicy::validated(FetchMode::Listed, &next) {
            Ok(_) => domains = next,
            Err(error) => {
                assert!(error.contains("too long to save"), "{error}");
                break;
            }
        }
    }
    assert!(domains.len() < MAX_DOMAINS, "the byte bound never applied");
    let policy = FetchPolicy::validated(FetchMode::Listed, &domains).unwrap();
    let saved = serde_json::to_vec(&policy).unwrap().len();
    assert!(saved <= MAX_POLICY_BYTES && MAX_POLICY_BYTES < MAX_BODY_BYTES);
    // "Always allow <host>" refuses growth past the bound and leaves the policy as it was.
    let error = policy.with_host(&long(domains.len())).unwrap_err();
    assert!(error.contains("too long to save"), "{error}");
    assert!(
        policy.with_host(&domains[0]).is_ok(),
        "no growth, no refusal"
    );
}

#[test]
fn local_and_unreadable_hosts_go_to_the_card_under_always_and_a_grant() {
    for url in [
        "http://169.254.169.254/latest/meta-data/",
        "http://2130706433/",
        "http://0x7f.1/",
        "http://127.1/",
        "http://[::1]:8080/",
        "http://[::ffff:127.0.0.1]/",
        "http://localhost/",
        "http://LOCALHOST./",
        "http://app.localhost:3000/",
        "http://intranet/",
        "ftp://example.com/",
        "not a url",
        "",
    ] {
        let view = fetch(url);
        for mode in [FetchMode::Ask, FetchMode::Listed, FetchMode::Always] {
            let rule = policy(mode, &["docs.rs"]).rule(&view, true);
            assert_eq!(rule, None, "{mode:?} {url}");
        }
        let never = policy(FetchMode::Never, &[]).rule(&view, true);
        assert_eq!(never, Some(FetchRule::Never), "{url}");
    }
    let public = fetch("https://_dmarc.example.com/");
    let always = policy(FetchMode::Always, &[]).rule(&public, false);
    assert_eq!(always, Some(FetchRule::Always));
}

#[test]
fn a_host_the_list_cannot_hold_is_not_listable_but_keeps_its_name() {
    let cases = [
        ("https://docs.rs/x", Some("docs.rs"), true),
        ("https://localhost:8080/", Some("localhost"), false),
        ("https://intranet/", Some("intranet"), false),
        ("https://app.localhost/", Some("app.localhost"), false),
        (
            "https://_dmarc.example.com/",
            Some("_dmarc.example.com"),
            false,
        ),
        ("https://127.0.0.1/", None, false),
    ];
    for (url, host, listable) in cases {
        let view = fetch(url);
        assert_eq!(view.host.as_deref(), host, "{url}");
        assert_eq!(view.listable, listable, "{url}");
    }
}

#[test]
fn the_view_shows_the_url_as_parsed_and_cut_to_a_bound() {
    let homograph = fetch("https://аpple.com/Path");
    assert_eq!(homograph.url, "https://xn--pple-43d.com/Path");
    assert_eq!(homograph.host.as_deref(), Some("xn--pple-43d.com"));
    assert_eq!(fetch("not a url").url, "not a url");
    let long = fetch(&format!("https://docs.rs/{}", "a".repeat(5000)));
    assert_eq!(long.url.chars().count(), MAX_SHOWN_URL_CHARS + 1);
    assert!(long.url.starts_with("https://docs.rs/aaa") && long.url.ends_with('…'));
    assert_eq!(long.host.as_deref(), Some("docs.rs"));
}

#[test]
fn a_listed_answer_names_the_entry_that_matched() {
    let list = policy(FetchMode::Listed, &["example.org", "docs.rs"]);
    let (rule, settled) = list
        .settle(&fetch("https://api.docs.rs/v1"), false)
        .unwrap();
    assert_eq!(rule, FetchRule::Listed);
    assert_eq!(settled.matched.as_deref(), Some("docs.rs"));
    assert_eq!(settled.rule, Some(FetchRule::Listed));
    let (rule, settled) = list.settle(&fetch("https://other.net/"), true).unwrap();
    assert_eq!((rule, settled.matched), (FetchRule::Session, None));
}

#[test]
fn listing_a_host_moves_ask_to_listed_and_keeps_the_other_modes() {
    let cases = [
        (FetchMode::Ask, FetchMode::Listed),
        (FetchMode::Listed, FetchMode::Listed),
        (FetchMode::Always, FetchMode::Always),
        (FetchMode::Never, FetchMode::Never),
    ];
    for (before, after) in cases {
        let grown = policy(before, &["a.example"]).with_host("docs.rs").unwrap();
        assert_eq!(grown.mode, after);
        assert_eq!(grown.domains, ["a.example", "docs.rs"]);
    }
}

#[test]
fn the_policy_refuses_an_unknown_field_on_the_wire() {
    let parsed = serde_json::from_value::<FetchPolicy>(
        json!({"mode": "always", "domains": [], "extra": true}),
    );
    assert!(parsed.is_err());
    let parsed: FetchPolicy =
        serde_json::from_value(json!({"mode": "listed", "domains": ["docs.rs"]})).unwrap();
    assert_eq!(parsed, policy(FetchMode::Listed, &["docs.rs"]));
}
