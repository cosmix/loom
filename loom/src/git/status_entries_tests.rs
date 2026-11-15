use super::*;

fn entry(code: &[u8; 2], path: &str, origin: Option<&str>) -> StatusEntry {
    StatusEntry {
        code: *code,
        path: path.to_owned(),
        origin: origin.map(str::to_owned),
    }
}

fn refusal(output: &[u8]) -> String {
    format!(
        "{:#}",
        parse_porcelain_v1_z(output).expect_err("listing must be refused")
    )
}

#[test]
fn empty_listing_has_no_entries() {
    assert_eq!(parse_porcelain_v1_z(b"").expect("empty parses"), vec![]);
}

#[test]
fn rename_and_copy_take_the_next_field_as_origin() {
    let entries =
        parse_porcelain_v1_z(b"R  new.rs\0old.rs\0 C copy.rs\0src.rs\0M  m.rs\0").expect("parses");
    assert_eq!(
        entries,
        vec![
            entry(b"R ", "new.rs", Some("old.rs")),
            entry(b" C", "copy.rs", Some("src.rs")),
            entry(b"M ", "m.rs", None),
        ]
    );
}

#[test]
fn paths_are_kept_verbatim() {
    let entries = parse_porcelain_v1_z(b"??  lead.rs\0 M a -> b.rs\0").expect("parses");
    assert_eq!(
        entries,
        vec![
            entry(b"??", " lead.rs", None),
            entry(b" M", "a -> b.rs", None),
        ]
    );
}

#[test]
fn untracked_and_ignored_codes() {
    let entries = parse_porcelain_v1_z(b"?? new.rs\0!! target/\0").expect("parses");
    assert!(entries[0].is_untracked());
    assert!(!entries[1].is_untracked());
    assert_eq!(entries[1].code_str(), "!!");
}

#[test]
fn columns_and_code_text() {
    let entries = parse_porcelain_v1_z(b"AM a.rs\0").expect("parses");
    assert_eq!(entries[0].index(), b'A');
    assert_eq!(entries[0].worktree(), b'M');
    assert_eq!(entries[0].code_str(), "AM");
    assert!(!entries[0].is_untracked());
}

#[test]
fn refuses_non_utf8() {
    let message = refusal(b"?? \xff.rs\0");
    assert!(
        message.starts_with("git status contains a non-UTF-8 path"),
        "{message}"
    );
}

#[test]
fn refuses_unterminated_listing() {
    assert_eq!(
        refusal(b" M a.rs"),
        "git status record lacks a terminating NUL"
    );
}

#[test]
fn refuses_malformed_headers() {
    for output in [&b"  a.rs\0"[..], b"XY a.rs\0", b" Ma.rs\0", b"M\0"] {
        let message = refusal(output);
        assert!(
            message == "invalid git status record header"
                || message == "git status record lacks an XY status",
            "{output:?}: {message}"
        );
    }
}

#[test]
fn refuses_missing_and_empty_paths() {
    assert_eq!(refusal(b" M\0"), "invalid git status record header");
    assert_eq!(refusal(b" M \0"), "git status path is empty");
}

#[test]
fn refuses_missing_and_empty_origins() {
    assert_eq!(
        refusal(b"R  new.rs\0"),
        "git status rename/copy lacks a source"
    );
    assert_eq!(
        refusal(b"R  new.rs\0\0"),
        "git status rename/copy source is empty"
    );
}
