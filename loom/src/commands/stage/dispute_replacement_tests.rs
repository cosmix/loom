//! `loom stage dispute-contract --replacement <frozen-file>=<path>`: what the
//! CLI accepts as a replacement before any ticket or request is built.

use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::Stage;
use crate::verify::contracts::test_support::{contract_worktree, CONTRACT_FILE};
use crate::verify::transitions::save_stage;

/// A repository with stage `s1`'s worktree and a scratch directory beside it:
/// `(temp dir, state directory, worktree, scratch)`.
fn site() -> (TempDir, PathBuf, PathBuf, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work = workspace.root().to_path_buf();
    let stage = Stage {
        id: "s1".to_string(),
        name: "s1".to_string(),
        plan_version: 2,
        worktree: Some("s1".to_string()),
        ..Stage::default()
    };
    save_stage(&stage, &work).unwrap();
    let scratch = tmp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    (tmp, work, worktree, scratch)
}

fn read(work: &Path, path: &Path) -> Result<String> {
    read_replacement(&stage_worktree_root(work, "s1")?, path)
}

fn contract_dispute(content: String) -> DisputeKind {
    DisputeKind::Contract {
        contract_ids: vec!["rejects-x".to_string()],
        replacements: vec![FileReplacement {
            file: CONTRACT_FILE.to_string(),
            content,
            base_sha256: None,
        }],
    }
}

fn read_all(work: &Path, values: &[String]) -> Result<Vec<FileReplacement>> {
    read_replacements(work, "s1", values)
}

#[test]
fn each_replacement_is_keyed_by_its_normalized_frozen_file() {
    let (_tmp, work, _worktree, scratch) = site();
    let path = scratch.join("x_contract.rs");
    std::fs::write(&path, "new\n").unwrap();
    let value = format!("./{CONTRACT_FILE}={}", path.display());

    let replacements = read_all(&work, &[value]).unwrap();

    assert_eq!(
        replacements,
        [FileReplacement {
            file: CONTRACT_FILE.to_string(),
            content: "new\n".to_string(),
            base_sha256: None,
        }]
    );
}

#[test]
fn a_replacement_value_not_of_the_form_file_eq_path_is_refused() {
    let (_tmp, work, _worktree, scratch) = site();
    let path = scratch.join("x_contract.rs").display().to_string();
    for value in [
        path.clone(),
        format!("={path}"),
        format!("{CONTRACT_FILE}="),
    ] {
        let error = format!(
            "{:#}",
            read_all(&work, std::slice::from_ref(&value)).unwrap_err()
        );
        assert!(error.contains("<frozen-file>=<path>"), "{value}: {error}");
    }
}

#[test]
fn a_frozen_file_replaced_twice_is_refused() {
    let (_tmp, work, _worktree, scratch) = site();
    let path = scratch.join("x_contract.rs");
    std::fs::write(&path, "new\n").unwrap();
    let value = format!("{CONTRACT_FILE}={}", path.display());

    let error = format!(
        "{:#}",
        read_all(&work, &[value.clone(), value]).unwrap_err()
    );

    assert!(error.contains("twice"), "{error}");
}

#[test]
fn a_replacement_outside_the_worktree_is_read_as_written() {
    let (_tmp, work, _worktree, scratch) = site();
    let path = scratch.join("x_contract.rs");
    std::fs::write(&path, "line\r\n---\n  tail  ").unwrap();

    assert_eq!(read(&work, &path).unwrap(), "line\r\n---\n  tail  ");
}

#[test]
fn a_40_kib_replacement_is_refused_by_the_byte_bound() {
    let (_tmp, work, _worktree, scratch) = site();
    let path = scratch.join("big.rs");
    std::fs::write(&path, "x".repeat(40 * 1024)).unwrap();

    let error = format!("{:#}", read(&work, &path).unwrap_err());

    assert!(error.contains("32768"), "{error}");
}

#[test]
fn a_replacement_that_is_not_utf8_is_refused() {
    let (_tmp, work, _worktree, scratch) = site();
    let path = scratch.join("binary.rs");
    std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap();

    let error = format!("{:#}", read(&work, &path).unwrap_err());

    assert!(error.contains("not valid UTF-8"), "{error}");
}

#[test]
fn a_path_inside_the_worktree_is_refused_however_it_is_spelled() {
    let (_tmp, work, worktree, scratch) = site();
    let frozen = worktree.join(CONTRACT_FILE);
    let dotted = worktree.join("tests/./../tests/x_contract.rs");
    let link = scratch.join("link.rs");
    std::os::unix::fs::symlink(&frozen, &link).unwrap();

    for path in [&frozen, &dotted, &link] {
        let error = format!("{:#}", read(&work, path).unwrap_err());
        assert!(error.contains("inside the stage worktree"), "{error}");
    }
}

#[test]
fn a_symlinked_replacement_is_refused_even_outside_the_worktree() {
    let (tmp, work, _worktree, scratch) = site();
    let real = tmp.path().join("real.rs");
    std::fs::write(&real, "outside\n").unwrap();
    let link = scratch.join("link.rs");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let error = format!("{:#}", read(&work, &link).unwrap_err());

    assert!(error.contains("a symlink is refused"), "{error}");
}

#[test]
fn a_replacement_within_the_byte_bound_can_still_exceed_the_ticket_bound() {
    // U+0001 is one byte and six when JSON-escaped: 31 KiB passes the byte
    // bound and encodes to about 186 KiB.
    let text = "\u{1}".repeat(31 * 1024);
    assert!(text.len() <= MAX_REPLACEMENT_BYTES);

    let error = ensure_fits_ticket(&contract_dispute(text), "why", None).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("32768"), "{message}");
    assert!(message.contains(&MAX_TICKET_BYTES.to_string()), "{message}");
    assert!(
        message.contains("separate filings, one file each"),
        "{message}"
    );
}

#[test]
fn a_plain_30_kib_replacement_fits_the_ticket() {
    let text = "let x = 1;\n".repeat(30 * 1024 / 11);

    ensure_fits_ticket(&contract_dispute(text), "why", None).unwrap();
}

#[test]
fn a_dispute_without_a_replacement_is_never_measured() {
    let kind = DisputeKind::Contract {
        contract_ids: vec!["rejects-x".to_string()],
        replacements: Vec::new(),
    };

    ensure_fits_ticket(&kind, &"r".repeat(10 * 1024 * 1024), None).unwrap();
}
