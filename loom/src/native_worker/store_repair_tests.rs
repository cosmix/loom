use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::super::tests::invocation;
use super::*;
use crate::native_worker::store::read_events;

fn sidecars(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("events.jsonl.torn-")
        })
        .collect()
}

fn append_raw(dir: &Path, bytes: &[u8]) {
    let mut file = OpenOptions::new()
        .append(true)
        .open(dir.join("events.jsonl"))
        .unwrap();
    file.write_all(bytes).unwrap();
}

#[test]
fn torn_tail_is_preserved_and_truncated() {
    let (_work, dir) = invocation();
    append_raw(&dir, br#"{"seq":2"#);
    assert!(read_events(&dir).is_err());
    assert!(repair_torn_tail(&dir).unwrap());
    assert_eq!(read_events(&dir).unwrap().len(), 1);
    let found = sidecars(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(fs::read(&found[0]).unwrap(), br#"{"seq":2"#);
}

#[test]
fn repair_refuses_a_corrupt_middle_line() {
    let (_work, dir) = invocation();
    let path = dir.join("events.jsonl");
    let mut body = b"broken\n".to_vec();
    body.extend_from_slice(&fs::read(&path).unwrap());
    body.extend_from_slice(br#"{"seq":3"#);
    fs::write(&path, &body).unwrap();
    let Err(StoreError::Corrupt(detail)) = repair_torn_tail(&dir) else {
        panic!("a corrupt middle line must be refused");
    };
    assert!(detail.starts_with("line 1:"), "{detail}");
    assert_eq!(fs::read(&path).unwrap(), body);
    assert!(sidecars(&dir).is_empty());
}

#[test]
fn repair_is_a_no_op_on_a_clean_or_newline_less_file() {
    let (_work, dir) = invocation();
    let path = dir.join("events.jsonl");
    let clean = fs::read(&path).unwrap();
    assert!(!repair_torn_tail(&dir).unwrap());
    assert_eq!(fs::read(&path).unwrap(), clean);
    fs::write(&path, br#"{"seq":1"#).unwrap();
    assert!(!repair_torn_tail(&dir).unwrap());
    assert_eq!(fs::read(&path).unwrap(), br#"{"seq":1"#);
    assert!(sidecars(&dir).is_empty());
}

#[test]
fn a_tail_torn_inside_a_multibyte_character_is_repaired() {
    let (_work, dir) = invocation();
    let mut torn = br#"{"seq":2,"reason":""#.to_vec();
    torn.push("\u{20ac}".as_bytes()[0]);
    append_raw(&dir, &torn);
    assert!(read_events(&dir).is_err());
    assert!(has_torn_tail(&dir));
    assert!(repair_torn_tail(&dir).unwrap());
    assert_eq!(read_events(&dir).unwrap().len(), 1);
    assert!(!has_torn_tail(&dir));
    let found = sidecars(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(fs::read(&found[0]).unwrap(), torn);
}

#[test]
fn invalid_utf8_before_the_cut_is_refused() {
    let (_work, dir) = invocation();
    let path = dir.join("events.jsonl");
    let mut body = vec![0xe2, b'\n'];
    body.extend_from_slice(&fs::read(&path).unwrap());
    body.extend_from_slice(br#"{"seq":3"#);
    fs::write(&path, &body).unwrap();
    assert!(matches!(
        repair_torn_tail(&dir),
        Err(StoreError::Corrupt(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), body);
    assert!(sidecars(&dir).is_empty());
}

#[test]
fn has_torn_tail_ignores_a_clean_or_empty_journal() {
    let (_work, dir) = invocation();
    assert!(!has_torn_tail(&dir));
    fs::write(dir.join("events.jsonl"), b"").unwrap();
    assert!(!has_torn_tail(&dir));
}
