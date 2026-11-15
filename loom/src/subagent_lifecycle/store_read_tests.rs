use super::{is_not_found, read_journal};

#[test]
fn read_journal_skips_blank_lines_and_rejects_malformed_complete_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lifecycle.jsonl");

    let missing = read_journal(&path).unwrap_err();
    assert!(is_not_found(&missing), "a missing journal is NotFound");

    std::fs::write(&path, "\n\n").unwrap();
    assert!(read_journal(&path).unwrap().is_empty());

    std::fs::write(&path, "{not json}\n").unwrap();
    let error = read_journal(&path).unwrap_err();
    assert!(format!("{error:#}").contains("malformed complete lifecycle line"));

    std::fs::write(&path, "{torn").unwrap();
    assert!(read_journal(&path).unwrap().is_empty());
}
