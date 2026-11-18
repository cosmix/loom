//! Tests for browser and public identifiers.

use super::ids::{BrowserId, PublicId};

#[test]
fn browser_id_parse_requires_exact_lowercase_hex() {
    let valid = "0123456789abcdef0123456789abcdef";
    assert_eq!(BrowserId::parse(valid).unwrap().as_str(), valid);
    assert!(BrowserId::parse(&valid[..31]).is_none());
    assert!(BrowserId::parse(&format!("{valid}0")).is_none());
    assert!(BrowserId::parse(&valid.to_uppercase()).is_none());
    assert!(BrowserId::parse("0123456789abcdef0123456789abcdeg").is_none());
}

#[test]
fn minted_browser_ids_are_distinct_and_debug_is_redacted() {
    let first = BrowserId::mint().unwrap();
    let second = BrowserId::mint().unwrap();
    assert_ne!(first, second);
    assert_eq!(first.as_str().len(), 32);
    assert_eq!(format!("{first:?}"), "BrowserId(..)");
    assert!(!format!("{first:?}").contains(first.as_str()));
}

#[test]
fn public_ids_are_distinct_and_serialize_as_lowercase_hex() {
    let first = PublicId::mint();
    let second = PublicId::mint();
    assert_ne!(first, second);
    let value = serde_json::to_value(&first).unwrap();
    let text = value.as_str().unwrap();
    assert_eq!(text.len(), 16);
    assert!(text
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(serde_json::from_value::<PublicId>(value).unwrap(), first);
}
