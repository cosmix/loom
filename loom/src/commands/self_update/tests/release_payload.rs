//! Tests for `tag_from_release_location`, which extracts the release tag
//! from the `Location` header GitHub returns when redirecting
//! `releases/latest` to `releases/tag/<tag>`.

use crate::commands::self_update::tag_from_release_location;

#[test]
fn resolves_an_absolute_location() {
    assert_eq!(
        tag_from_release_location("https://github.com/cosmix/loom/releases/tag/v1.2.3").unwrap(),
        "v1.2.3"
    );
}

#[test]
fn resolves_a_relative_location() {
    assert_eq!(
        tag_from_release_location("/cosmix/loom/releases/tag/v0.8.2").unwrap(),
        "v0.8.2"
    );
}

#[test]
fn rejects_a_location_without_the_tag_marker() {
    assert!(tag_from_release_location("https://github.com/cosmix/loom/releases").is_err());
}

#[test]
fn rejects_an_empty_tag() {
    assert!(tag_from_release_location("https://github.com/cosmix/loom/releases/tag/").is_err());
}

#[test]
fn rejects_a_tag_containing_a_slash() {
    assert!(tag_from_release_location("https://github.com/cosmix/loom/releases/tag/v1/2").is_err());
}
