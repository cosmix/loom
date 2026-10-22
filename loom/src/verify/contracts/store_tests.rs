//! Tests for the freeze store: the record, the frozen copies, the attempts
//! budget, and each contract's recorded run directory.

use super::*;
use tempfile::TempDir;

fn record(stage_id: &str, path: &str, sha256: &str) -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: stage_id.to_string(),
        session_id: "session-1".to_string(),
        frozen_at: Utc::now(),
        base: "a".repeat(40),
        files: vec![FrozenFile {
            path: path.to_string(),
            sha256: sha256.to_string(),
        }],
        contracts: Vec::new(),
    }
}

#[test]
fn write_then_load_round_trips_and_refuses_a_second_freeze() {
    let tmp = TempDir::new().unwrap();
    let bytes = b"fn contract() {}\n".to_vec();
    let frozen = record("s1", "tests/a_contract.rs", "abc");
    let files = vec![("tests/a_contract.rs".to_string(), bytes.clone())];

    write_freeze(tmp.path(), &frozen, &files).unwrap();

    assert_eq!(load_freeze(tmp.path(), "s1").unwrap(), Some(frozen.clone()));
    let copy = frozen_file_path(tmp.path(), "s1", "tests/a_contract.rs");
    assert_eq!(std::fs::read(copy).unwrap(), bytes);
    let again = write_freeze(tmp.path(), &frozen, &files).unwrap_err();
    assert!(again.to_string().contains("already frozen"), "{again:#}");
}

#[test]
fn a_stage_without_a_freeze_loads_none() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(load_freeze(tmp.path(), "s1").unwrap(), None);
}

#[test]
fn escaping_paths_and_ids_are_refused() {
    let tmp = TempDir::new().unwrap();
    for path in ["../x.rs", "/etc/passwd", "a/../../x", "./a.rs", ""] {
        assert!(validate_relative(path).is_err(), "{path}");
        let files = vec![(path.to_string(), Vec::new())];
        assert!(write_freeze(tmp.path(), &record("s1", "a.rs", "x"), &files).is_err());
    }
    assert!(load_freeze(tmp.path(), "../escape").is_err());
    assert!(spend_attempt(tmp.path(), "../escape").is_err());
}

#[test]
fn attempts_are_counted_as_they_are_spent() {
    let tmp = TempDir::new().unwrap();
    assert_eq!(attempts_spent(tmp.path(), "s1").unwrap(), 0);
    assert_eq!(spend_attempt(tmp.path(), "s1").unwrap(), 1);
    assert_eq!(spend_attempt(tmp.path(), "s1").unwrap(), 2);
    assert_eq!(attempts_spent(tmp.path(), "s1").unwrap(), 2);
}

#[test]
fn a_reset_refills_the_budget_until_the_contracts_are_frozen() {
    let tmp = TempDir::new().unwrap();
    spend_attempt(tmp.path(), "s1").unwrap();
    spend_attempt(tmp.path(), "s1").unwrap();
    reset_attempts(tmp.path(), "s1").unwrap();
    assert_eq!(attempts_spent(tmp.path(), "s1").unwrap(), 0);

    spend_attempt(tmp.path(), "s1").unwrap();
    write_freeze(tmp.path(), &record("s1", "a.rs", "x"), &[]).unwrap();
    reset_attempts(tmp.path(), "s1").unwrap();
    assert_eq!(attempts_spent(tmp.path(), "s1").unwrap(), 1);
}

fn red(cwd: Option<&str>) -> FrozenContract {
    FrozenContract {
        id: "rejects-x".to_string(),
        adapter: Some("bun-test".to_string()),
        outcome: "failed".to_string(),
        exit_code: Some(1),
        file: None,
        cwd: cwd.map(str::to_string),
    }
}

#[test]
fn a_recorded_run_dir_must_be_the_root_or_a_plain_relative_path() {
    for cwd in [".", "apps/web"] {
        assert!(validate_cwd(cwd).is_ok(), "{cwd}");
    }
    for cwd in ["", "./apps", "../apps", "/apps", "apps/../x"] {
        assert!(validate_cwd(cwd).is_err(), "{cwd}");
    }
    let tmp = TempDir::new().unwrap();
    let mut frozen = record("s1", "a.rs", "x");
    frozen.contracts = vec![red(Some("../outside"))];
    write_freeze(tmp.path(), &frozen, &[]).unwrap();
    let error = load_freeze(tmp.path(), "s1").unwrap_err();
    assert!(format!("{error:#}").contains("run directory"), "{error:#}");
}

#[test]
fn a_record_without_run_dirs_still_loads() {
    let tmp = TempDir::new().unwrap();
    let mut frozen = record("s1", "a.rs", "x");
    frozen.contracts = vec![red(None)];
    write_freeze(tmp.path(), &frozen, &[]).unwrap();
    let json = std::fs::read_to_string(tmp.path().join("contracts/s1/freeze.json")).unwrap();
    assert!(!json.contains("cwd"), "{json}");

    assert_eq!(load_freeze(tmp.path(), "s1").unwrap(), Some(frozen));
}

#[test]
fn a_refreeze_records_each_contracts_run_dir() {
    let tmp = TempDir::new().unwrap();
    let mut frozen = record("s1", "a.rs", "x");
    frozen.contracts = vec![red(None)];
    write_freeze(tmp.path(), &frozen, &[]).unwrap();
    let files = vec![("a.rs".to_string(), b"new".to_vec())];

    refreeze_set(
        tmp.path(),
        "s1",
        &files,
        &[refrozen("rejects-x", "apps/web")],
    )
    .unwrap();

    let loaded = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    let updated = FrozenContract {
        file: Some("a.rs".to_string()),
        ..red(Some("apps/web"))
    };
    assert_eq!(loaded.contracts, vec![updated]);
    assert_eq!(loaded.files[0].sha256, sha256_hex(b"new"));
    let bad = [refrozen("rejects-x", "../x")];
    assert!(refreeze_set(tmp.path(), "s1", &files, &bad).is_err());
}

#[test]
fn a_refreeze_adds_a_contract_the_record_did_not_list() {
    let tmp = TempDir::new().unwrap();
    let mut frozen = record("s1", "a.rs", "x");
    frozen.contracts = vec![red(Some("."))];
    write_freeze(tmp.path(), &frozen, &[]).unwrap();

    refreeze_set(tmp.path(), "s1", &[], &[refrozen("rejects-y", "apps/web")]).unwrap();

    let loaded = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    let added = FrozenContract {
        id: "rejects-y".to_string(),
        adapter: Some("bun-test".to_string()),
        outcome: REFROZEN_OUTCOME.to_string(),
        exit_code: None,
        file: Some("a.rs".to_string()),
        cwd: Some("apps/web".to_string()),
    };
    assert_eq!(loaded.contracts, vec![red(Some(".")), added]);
}

#[test]
fn a_refreeze_replaces_the_frozen_set_and_drops_what_it_lacks() {
    let tmp = TempDir::new().unwrap();
    let mut frozen = record("s1", "a.rs", &sha256_hex(b"old"));
    frozen.files.push(FrozenFile {
        path: "gone.rs".to_string(),
        sha256: sha256_hex(b"gone"),
    });
    let copies = [
        ("a.rs".to_string(), b"old".to_vec()),
        ("gone.rs".to_string(), b"gone".to_vec()),
    ];
    write_freeze(tmp.path(), &frozen, &copies).unwrap();
    let files = [
        ("a.rs".to_string(), b"new".to_vec()),
        ("added.rs".to_string(), b"added".to_vec()),
    ];

    let dropped = refreeze_set(tmp.path(), "s1", &files, &[]).unwrap();

    assert_eq!(dropped, vec!["gone.rs".to_string()]);
    let loaded = load_freeze(tmp.path(), "s1").unwrap().unwrap();
    let hashes: Vec<(&str, String)> = loaded
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.sha256.clone()))
        .collect();
    let expected = vec![
        ("a.rs", sha256_hex(b"new")),
        ("added.rs", sha256_hex(b"added")),
    ];
    assert_eq!(hashes, expected);
    assert!(!frozen_file_path(tmp.path(), "s1", "gone.rs").exists());
    let added = std::fs::read(frozen_file_path(tmp.path(), "s1", "added.rs")).unwrap();
    assert_eq!(added, b"added");
}

fn refrozen(id: &str, cwd: &str) -> RefrozenContract {
    RefrozenContract {
        id: id.to_string(),
        adapter: Some("bun-test".to_string()),
        file: "a.rs".to_string(),
        cwd: cwd.to_string(),
    }
}

/// A record entry for `id`, frozen with `file` at `cwd`.
fn entry(id: &str, file: Option<&str>, cwd: Option<&str>) -> FrozenContract {
    FrozenContract {
        id: id.to_string(),
        file: file.map(str::to_string),
        ..red(cwd)
    }
}

/// The cwd [`recorded_cwd`] finds for a contract `id` naming `file`.
fn recorded(entries: Vec<FrozenContract>, id: &str, file: &str) -> Option<String> {
    let mut frozen = record("s1", "a.rs", "x");
    frozen.contracts = entries;
    let contract = ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        ..crate::verify::contracts::test_support::contract()
    };
    recorded_cwd(&frozen, &contract).map(str::to_string)
}

#[test]
fn a_recorded_cwd_counts_only_for_the_file_it_was_recorded_with() {
    let own = || entry("x", Some("a.rs"), Some("own"));
    let other = || entry("y", Some("a.rs"), Some("other"));
    assert_eq!(
        recorded(vec![other(), own()], "x", "./a.rs").as_deref(),
        Some("own")
    );
    assert_eq!(
        recorded(vec![other()], "x", "a.rs").as_deref(),
        Some("other")
    );
    assert_eq!(recorded(vec![own()], "x", "b.rs"), None);
    let legacy = entry("x", None, Some("legacy"));
    assert_eq!(
        recorded(vec![legacy.clone()], "x", "b.rs").as_deref(),
        Some("legacy")
    );
    assert_eq!(recorded(vec![legacy], "y", "b.rs"), None);
    assert_eq!(
        recorded(vec![entry("x", Some("a.rs"), None)], "x", "a.rs"),
        None
    );
}

#[test]
fn a_recorded_file_must_be_a_plain_relative_path() {
    let tmp = TempDir::new().unwrap();
    let mut frozen = record("s1", "a.rs", "x");
    frozen.contracts = vec![entry("x", Some("../a.rs"), Some("."))];
    write_freeze(tmp.path(), &frozen, &[]).unwrap();

    assert!(load_freeze(tmp.path(), "s1").is_err());
    write_freeze(tmp.path(), &record("s2", "a.rs", "x"), &[]).unwrap();
    let bad = RefrozenContract {
        file: "/a.rs".to_string(),
        ..refrozen("x", ".")
    };
    assert!(refreeze_set(tmp.path(), "s2", &[], &[bad]).is_err());
}
