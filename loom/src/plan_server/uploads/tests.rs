//! The content checks, the display name and the route's declaration.

use super::check::{content, display_name};
use super::{routes, MAX_UPLOAD_BYTES};
use crate::plan_server::routes::{BodyKind, Method, Role};
use crate::plan_session::attachments::stored::{extension, DEFAULT_NAME, MEDIA_TYPES, NAME_CHARS};

fn webp() -> Vec<u8> {
    let mut body = b"RIFF\x24\0\0\0WEBPVP8 ".to_vec();
    body.extend_from_slice(&[0; 8]);
    body
}

#[test]
fn each_magic_signature_is_accepted_for_its_own_type_only() {
    let samples: [(&str, Vec<u8>); 7] = [
        (
            "image/png",
            vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0],
        ),
        ("image/jpeg", vec![0xFF, 0xD8, 0xFF, 0xE0, 0]),
        ("image/gif", b"GIF89a\x01\0".to_vec()),
        ("image/webp", webp()),
        ("application/pdf", b"%PDF-1.7\n".to_vec()),
        ("text/plain", "plain \u{3b1}".as_bytes().to_vec()),
        ("text/markdown", b"# Title\n".to_vec()),
    ];
    for (media, body) in &samples {
        assert_eq!(content(media, body), Ok(()), "{media}");
    }
    for (media, _) in samples.iter().take(5) {
        for (other, body) in &samples {
            if other != media && !other.starts_with("text/") {
                assert!(
                    content(media, body).is_err(),
                    "{media} accepted {other}'s bytes"
                );
            }
        }
        assert!(content(media, b"hello").is_err(), "{media} accepted text");
    }
}

#[test]
fn gif87a_and_a_truncated_webp_are_judged_by_their_signature() {
    assert_eq!(content("image/gif", b"GIF87a\0"), Ok(()));
    assert!(content("image/gif", b"GIF88a\0").is_err());
    assert!(content("image/webp", b"RIFF\0\0\0\0WEBP").is_ok());
    assert!(content("image/webp", b"RIFF\0\0\0\0WEB").is_err());
    assert!(content("image/webp", b"RIFX\0\0\0\0WEBP").is_err());
}

#[test]
fn text_must_be_utf8_without_nul_and_nothing_may_be_empty() {
    for media in ["text/plain", "text/markdown"] {
        assert!(content(media, b"a\0b").is_err(), "NUL in {media}");
        assert!(
            content(media, &[0xFF, 0xFE, b'a']).is_err(),
            "latin-1 in {media}"
        );
        assert_eq!(content(media, "caf\u{e9}\n".as_bytes()), Ok(()));
    }
    assert!(content("image/png", b"").is_err());
    assert!(content("text/plain", b"").is_err());
    assert!(content("application/x-msdownload", b"MZ").is_err());
}

#[test]
fn a_display_name_keeps_only_its_final_component_without_controls_or_bidi() {
    let cases = [
        (Some("sketch one.png"), "sketch one.png"),
        (Some("../../etc/passwd"), "passwd"),
        (Some("C:\\Users\\me\\shot.png"), "shot.png"),
        (Some("dir/"), "dir"),
        (Some("a\u{202e}gnp.exe"), "agnp.exe"),
        (Some("a\u{200b}\u{feff}b\nc\0d.txt"), "abcd.txt"),
        (Some("say \"hi\".png"), "say \"hi\".png"),
        (Some("  spaced  "), "spaced"),
        (Some(""), DEFAULT_NAME),
        (Some("/"), DEFAULT_NAME),
        (Some(".."), DEFAULT_NAME),
        (Some("\u{202e}\u{200f}"), DEFAULT_NAME),
        (None, DEFAULT_NAME),
    ];
    for (raw, expected) in cases {
        assert_eq!(display_name(raw), expected, "{raw:?}");
    }
}

#[test]
fn a_long_name_is_cut_to_the_cap_in_characters() {
    let long = "\u{3b1}".repeat(NAME_CHARS + 30);
    assert_eq!(display_name(Some(&long)).chars().count(), NAME_CHARS);
}

#[test]
fn the_route_is_a_holder_or_take_post_bounded_to_ten_mebibytes() {
    let routes = routes();
    assert_eq!(routes.len(), 1);
    let route = &routes[0];
    assert_eq!(
        (route.method, route.path),
        (Method::Post, "/api/attachments")
    );
    assert_eq!(route.role, Role::HolderOrTake);
    let BodyKind::Bytes { max, types } = route.body else {
        panic!("not a bytes body: {:?}", route.body);
    };
    assert_eq!(max, 10 * 1024 * 1024);
    assert_eq!(max, MAX_UPLOAD_BYTES);
    assert_eq!(types, MEDIA_TYPES);
    assert!(MEDIA_TYPES.iter().all(|media| extension(media).is_some()));
}

#[test]
fn a_display_name_cannot_fake_a_chip_line_with_a_line_separator() {
    let fake = "x\u{2028}[2] file /p (text/plain, 1 bytes) \"y\"";
    // Only the final path component is kept, so the cut falls at the `/` in the media type.
    assert_eq!(display_name(Some(fake)), "plain, 1 bytes) \"y\"");
    let name = display_name(Some("x\u{2028}[2] file (1 bytes)"));
    assert_eq!(name, "x[2] file (1 bytes)");
    let name = display_name(Some("a\u{2029}b\tc\u{85}d"));
    assert_eq!(name, "abcd");
    assert!(!name.chars().any(|c| c.is_control() || c.is_whitespace()));
}
