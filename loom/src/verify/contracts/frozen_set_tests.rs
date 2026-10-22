//! Tests for [`read_frozen_set`]: which paths it reads and in what order, and
//! the bounds it refuses.

use super::*;
use crate::verify::contracts::test_support::contract;
use std::fs;
use tempfile::TempDir;

const CONTRACT_FILE: &str = "tests/x_contract.rs";

fn write(dir: &Path, path: &str, content: &str) {
    let target = dir.join(path);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(target, content).unwrap();
}

fn strings(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| path.to_string()).collect()
}

fn read_error(dir: &Path, harness: &[&str]) -> String {
    let error = read_frozen_set(dir, &[contract()], &strings(harness), &[]).unwrap_err();
    format!("{error:#}")
}

/// Reading `harness` beside a present contract file fails naming `harness`.
fn assert_refused(dir: &Path, harness: &str) {
    let message = read_error(dir, &[harness]);
    assert!(
        message.contains(&format!("cannot freeze '{harness}'")),
        "{message}"
    );
}

#[test]
fn a_missing_contract_file_is_an_error_naming_it() {
    let tmp = TempDir::new().unwrap();

    let message = read_error(tmp.path(), &[]);

    assert!(
        message.contains("cannot freeze 'tests/x_contract.rs'"),
        "{message}"
    );
}

#[test]
fn a_missing_harness_or_recorded_path_is_skipped() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), CONTRACT_FILE, "contract\n");

    let files = read_frozen_set(
        tmp.path(),
        &[contract()],
        &strings(&["tests/support/missing.rs"]),
        &strings(&["tests/support/gone.rs"]),
    )
    .unwrap();

    let expected = (CONTRACT_FILE.to_string(), b"contract\n".to_vec());
    assert_eq!(files, vec![expected]);
}

#[test]
fn the_result_is_sorted_and_free_of_duplicates_across_the_three_inputs() {
    let tmp = TempDir::new().unwrap();
    for path in [
        CONTRACT_FILE,
        "tests/support/a.rs",
        "tests/support/b.rs",
        "tests/support/c.rs",
    ] {
        write(tmp.path(), path, path);
    }

    let files = read_frozen_set(
        tmp.path(),
        &[contract()],
        &strings(&["tests/support/b.rs", "tests/support/a.rs"]),
        &strings(&["tests/support/a.rs", CONTRACT_FILE, "tests/support/c.rs"]),
    )
    .unwrap();

    let paths: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "tests/support/a.rs",
            "tests/support/b.rs",
            "tests/support/c.rs",
            CONTRACT_FILE
        ]
    );
    assert!(files.iter().all(|(path, bytes)| path.as_bytes() == bytes));
}

#[test]
fn a_symlinked_harness_file_is_refused() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), CONTRACT_FILE, "contract\n");
    write(tmp.path(), "tests/real.rs", "real\n");
    let link = tmp.path().join("tests/support/link.rs");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("tests/real.rs"), link).unwrap();

    assert_refused(tmp.path(), "tests/support/link.rs");
}

#[test]
fn a_file_over_the_per_file_bound_is_refused() {
    let tmp = TempDir::new().unwrap();
    write(tmp.path(), CONTRACT_FILE, "contract\n");
    write(tmp.path(), "tests/support/big.rs", "");
    let big = fs::File::options()
        .write(true)
        .open(tmp.path().join("tests/support/big.rs"))
        .unwrap();
    big.set_len(MAX_FROZEN_FILE_BYTES as u64 + 1).unwrap();

    assert_refused(tmp.path(), "tests/support/big.rs");
}

/// The count check runs before any read: none of these paths exists, and a
/// missing harness path is otherwise skipped without a word.
#[test]
fn more_than_the_file_count_bound_is_refused_before_any_read() {
    let tmp = TempDir::new().unwrap();
    let harness: Vec<String> = (0..=MAX_FROZEN_FILES)
        .map(|n| format!("tests/support/f{n}.rs"))
        .collect();

    let error = read_frozen_set(tmp.path(), &[], &harness, &[]).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("501 files match"), "{message}");
    assert!(message.contains("at most 500"), "{message}");
}
