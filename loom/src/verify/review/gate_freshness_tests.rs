use super::*;

#[test]
fn multiline_string_edits_and_legacy_normalized_evidence_are_stale() {
    for (path, before) in [
        ("a.rs", "let s = r#\"\n// old\n\"#;\n"),
        ("a.js", "const s = `\n// old\n`;\n"),
        ("a.ts", "const s = `\n// old\n`;\n"),
        ("a.py", "s = '''\n# old\n'''\n"),
        ("a.go", "var s = `\n// old\n`\n"),
    ] {
        let reviewed = snapshot(path, Some(before));
        let seen = round(1, &reviewed, &[], &[]);
        for after in [
            before.replace("old", "new"),
            before.replace("old\n", "old\n\n"),
        ] {
            assert!(
                !covers_review(&seen, &snapshot(path, Some(&after))),
                "{path}"
            );
        }
        assert!(covers_review(&seen, &reviewed), "{path}");
    }
    let before = snapshot("doc/x.md", Some("old"));
    let mut legacy = round(1, &before, &[], &[]);
    for value in legacy.behaviour.values_mut() {
        *value = value.strip_prefix("v2:").unwrap().to_string();
    }
    assert!(!covers_review(&legacy, &snapshot("doc/x.md", Some("new"))));
    assert!(covers_review(&legacy, &before));
}
