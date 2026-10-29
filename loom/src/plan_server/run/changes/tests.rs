use super::{parse_changes, ChangedPath};

fn change(path: &str, status: &str) -> ChangedPath {
    ChangedPath {
        path: path.to_owned(),
        status: status.to_owned(),
    }
}

#[test]
fn empty_status_has_no_changes() {
    assert_eq!(parse_changes(b"").unwrap(), (vec![], vec![]));
}

#[test]
fn doc_changes_include_every_status_and_preserve_xy() {
    for status in [
        " M", "M ", "MM", "A ", " D", "D ", " T", "T ", "UU", "??", "!!",
    ] {
        let input = format!("{status} doc/file.md\0");
        let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
        assert_eq!(docs, vec![change("doc/file.md", status)]);
        assert!(blocking.is_empty(), "{status}");
    }
}

#[test]
fn tracked_changes_outside_doc_block_a_run() {
    for status in [" M", "M ", "MM", "A ", " D", "D ", " T", "T ", "UU"] {
        let input = format!("{status} src/file.rs\0");
        let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
        assert!(docs.is_empty(), "{status}");
        assert_eq!(blocking, vec![change("src/file.rs", status)]);
    }
}

#[test]
fn untracked_and_ignored_paths_outside_doc_do_not_block() {
    let (docs, blocking) = parse_changes(b"?? notes.txt\0!! target/output\0").unwrap();
    assert!(docs.is_empty());
    assert!(blocking.is_empty());
}

#[test]
fn docs_prefix_and_bare_doc_are_blocking() {
    let (docs, blocking) = parse_changes(b" M docs/x.md\0 M doc\0").unwrap();
    assert!(docs.is_empty());
    assert_eq!(
        blocking,
        vec![change("docs/x.md", " M"), change("doc", " M")]
    );
}

#[test]
fn absolute_and_dot_dot_paths_never_enter_doc_changes() {
    for path in [
        "/doc/x.md",
        "../doc/x.md",
        "doc/../src/x.rs",
        "doc/nested/../../x",
        "doc/a..b.md",
    ] {
        for status in [" M", "??"] {
            let input = format!("{status} {path}\0");
            let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
            assert!(docs.is_empty(), "{path}");
            assert_eq!(blocking.len(), usize::from(status != "??"));
        }
    }
}

#[test]
fn rename_into_doc_blocks_on_its_tracked_source_outside_doc() {
    let (docs, blocking) = parse_changes(b"R  doc/lib.rs\0src/lib.rs\0").unwrap();
    assert_eq!(docs, vec![change("doc/lib.rs", "R ")]);
    assert_eq!(blocking, vec![change("src/lib.rs", "R ")]);
}

#[test]
fn copy_into_doc_does_not_block_on_its_unchanged_source() {
    let (docs, blocking) = parse_changes(b"C  doc/lib.rs\0src/lib.rs\0").unwrap();
    assert_eq!(docs, vec![change("doc/lib.rs", "C ")]);
    assert!(blocking.is_empty());
}

#[test]
fn rename_out_of_doc_lists_source_and_blocks_on_destination() {
    let (docs, blocking) = parse_changes(b"R  src/new.md\0doc/old.md\0").unwrap();
    assert_eq!(docs, vec![change("doc/old.md", "R ")]);
    assert_eq!(blocking, vec![change("src/new.md", "R ")]);
}

#[test]
fn rename_and_copy_in_either_column_consume_two_paths() {
    for status in ["R ", " R", "RM", "MR", "C ", " C", "CM", "MC"] {
        let input = format!("{status} doc/new.md\0doc/old.md\0 M src/next.rs\0");
        let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
        assert_eq!(
            docs,
            vec![change("doc/new.md", status), change("doc/old.md", status)]
        );
        assert_eq!(blocking, vec![change("src/next.rs", " M")]);
    }
}

#[test]
fn rename_outside_doc_blocks_on_both_paths() {
    let (docs, blocking) = parse_changes(b" R src/new.rs\0src/old.rs\0").unwrap();
    assert!(docs.is_empty());
    assert_eq!(
        blocking,
        vec![change("src/new.rs", " R"), change("src/old.rs", " R")]
    );
}

#[test]
fn rename_sources_obey_doc_security_boundary() {
    for source in ["/doc/old.md", "doc/../old.md", "docs/old.md", "doc"] {
        let input = format!("R  doc/new.md\0{source}\0");
        let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
        assert_eq!(docs, vec![change("doc/new.md", "R ")]);
        assert_eq!(blocking, vec![change(source, "R ")]);
    }
}

#[test]
fn deleted_tracked_file_outside_doc_blocks() {
    let (docs, blocking) = parse_changes(b" D deleted.txt\0").unwrap();
    assert!(docs.is_empty());
    assert_eq!(blocking, vec![change("deleted.txt", " D")]);
}

#[test]
fn summary_temps_are_excluded_at_any_depth_for_every_status() {
    for status in ["??", " M", "A ", "D "] {
        let input = format!(
            "{status} doc/loom/summaries/.seal.tmp-1\0\
             {status} doc/loom/summaries/nested/deep/.seal.tmp-2\0"
        );
        let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
        assert!(docs.is_empty());
        assert!(blocking.is_empty());
    }
}

#[test]
fn summary_temp_exclusion_does_not_hide_other_files() {
    let paths = [
        "doc/loom/summaries/seal.tmp-1",
        "doc/loom/summaries/.seal.tmp",
        "doc/loom/summaries/.tmp-1",
        "doc/loom/summaries/.hidden.md",
        "doc/loom/summaries/.nested.tmp-1/report.md",
        "doc/loom/summaries-other/.seal.tmp-1",
        "doc/other/.seal.tmp-1",
    ];
    for path in paths {
        let input = format!("?? {path}\0");
        let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
        assert_eq!(docs, vec![change(path, "??")]);
        assert!(blocking.is_empty());
    }
}

#[test]
fn summary_temp_rename_sources_and_destinations_are_excluded() {
    let input = b"R  doc/new.md\0doc/loom/summaries/.old.tmp-1\0\
                  R  doc/loom/summaries/nested/.new.tmp-2\0doc/old.md\0";
    let (docs, blocking) = parse_changes(input).unwrap();
    assert_eq!(
        docs,
        vec![change("doc/new.md", "R "), change("doc/old.md", "R ")]
    );
    assert!(blocking.is_empty());
}

#[test]
fn nul_records_preserve_whitespace_quotes_arrows_and_unicode() {
    let paths = ["doc/ space \n\t\" -> é.md", "doc/ M source.md"];
    let input = format!("R  {}\0{}\0?? doc/last.md\0", paths[0], paths[1]);
    let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
    assert_eq!(
        docs,
        vec![
            change(paths[0], "R "),
            change(paths[1], "R "),
            change("doc/last.md", "??"),
        ]
    );
    assert!(blocking.is_empty());
}

#[test]
fn retains_three_hundred_doc_paths_and_following_blocker() {
    let mut input = String::new();
    let mut expected = Vec::new();
    for i in 0..300 {
        let path = format!("doc/many/f{i:03}.md");
        input.push_str(&format!("?? {path}\0"));
        expected.push(change(&path, "??"));
    }
    input.push_str(" M src/last.rs\0?? notes.txt\0");
    let (docs, blocking) = parse_changes(input.as_bytes()).unwrap();
    assert_eq!(docs, expected);
    assert_eq!(blocking, vec![change("src/last.rs", " M")]);
}

#[test]
fn malformed_records_fail_instead_of_returning_a_partial_allowlist() {
    for bad in [
        " M doc/unterminated.md",
        "\0",
        "M\0",
        " M\0",
        " M \0",
        " Mxdoc/x.md\0",
        "ZZ doc/x.md\0",
        "  doc/x.md\0",
        "R  doc/new.md\0",
        " C doc/new.md\0\0",
        "R  doc/new.md\0doc/old.md",
        "é doc/x.md\0",
    ] {
        let input = format!("?? doc/good.md\0{bad}");
        assert!(parse_changes(input.as_bytes()).is_err(), "{bad:?}");
    }
}

#[test]
fn non_utf8_paths_fail_instead_of_using_replacement_characters() {
    assert!(parse_changes(b" M doc/\xff.md\0").is_err());
    assert!(parse_changes(b"R  doc/new.md\0doc/\xff.md\0").is_err());
}
