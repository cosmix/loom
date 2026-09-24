//! Test-integrity events and the gate (DESIGN D13), each against a temporary
//! repository with a base commit on `main` and the stage's work on a branch.

use super::*;
use crate::git::runner::run_git_checked;
use crate::models::stage::{Stage, StageType};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tempfile::TempDir;

const TESTS_FILE: &str = "src/calc_tests.rs";
const STAGE: &str = "s1";

struct Repo {
    tmp: TempDir,
    root: PathBuf,
}

impl Repo {
    /// `files` committed on `main`, then branch `feature` checked out.
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let repo = Self { tmp, root };
        repo.git(&["init", "-q", "-b", "main"]);
        for &(path, content) in files {
            repo.write(path, content);
            repo.git(&["add", "--", path]);
        }
        repo.commit("base");
        repo.git(&["switch", "-q", "-c", "feature"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        run_git_checked(args, &self.root).unwrap()
    }

    fn commit(&self, message: &str) {
        self.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-am",
            message,
        ]);
    }

    fn write(&self, path: &str, content: &str) {
        let file = self.root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    fn events(&self, ratchet_files: &[String]) -> Vec<IntegrityEvent> {
        current_events(&self.root, "main", ratchet_files).unwrap()
    }
}

fn ids(events: &[IntegrityEvent]) -> Vec<&str> {
    events.iter().map(|event| event.id.as_str()).collect()
}

fn sha256_hex(content: &str) -> String {
    hex::encode(Sha256::digest(content.as_bytes()))
}

/// A Rust test file with one test holding `count` distinct assertions.
fn asserting(count: usize) -> String {
    let lines: String = (1..=count)
        .map(|check| format!("    assert!(n > 0, \"check {check}\");\n"))
        .collect();
    format!("#[test]\nfn counts() {{\n    let n = 1;\n{lines}}}\n")
}

#[test]
fn deleted_test_declaration_is_an_integrity_event() {
    let base = "#[test]\nfn adds() {}\n\n#[test]\nfn subtracts() {}\n";
    let repo = Repo::new(&[(TESTS_FILE, base)]);
    repo.write(TESTS_FILE, "#[test]\nfn adds() {}\n");
    repo.commit("drop a test");

    let expected = IntegrityEvent {
        id: "TI-decl-rust".to_string(),
        kind: EventKind::DeclTotal,
        language: Some("rust".to_string()),
        path: None,
        base: Some(2),
        current: Some(1),
        current_sha256: None,
        detail: Vec::new(),
    };
    assert_eq!(repo.events(&[]), vec![expected]);
}

#[test]
fn removed_assertion_is_an_integrity_event() {
    let base = "#[test]\nfn adds() {\n    assert_eq!(2 + 2, 4);\n    assert!(1 < 2);\n}\n";
    let repo = Repo::new(&[(TESTS_FILE, base)]);
    repo.write(
        TESTS_FILE,
        "#[test]\nfn adds() {\n    assert_eq!(2 + 2, 4);\n}\n",
    );

    let events = repo.events(&[]);
    let edit = format!("TI-edit-{TESTS_FILE}");
    assert_eq!(ids(&events), ["TI-assert-rust", edit.as_str()]);
    let total = &events[0];
    assert_eq!(total.kind, EventKind::AssertTotal);
    assert_eq!(total.language.as_deref(), Some("rust"));
    assert_eq!((total.base, total.current), (Some(2), Some(1)));
    assert_eq!(events[1].detail, ["assert!(1 < 2);"]);
}

#[test]
fn edited_assertion_in_base_test_is_an_integrity_event() {
    let base = "#[test]\nfn five() {\n    let x = 5;\n    assert_eq!(x, 5);\n}\n";
    let edited = base.replace("assert_eq!(x, 5)", "assert_eq!(x, 6)");
    let repo = Repo::new(&[(TESTS_FILE, base)]);
    repo.write(TESTS_FILE, &edited);

    let expected = IntegrityEvent {
        id: format!("TI-edit-{TESTS_FILE}"),
        kind: EventKind::AssertionEdit,
        language: Some("rust".to_string()),
        path: Some(TESTS_FILE.to_string()),
        base: None,
        current: None,
        current_sha256: Some(sha256_hex(&edited)),
        detail: vec!["assert_eq!(x, 5);".to_string()],
    };
    assert_eq!(repo.events(&[]), vec![expected]);
}

#[test]
fn moved_assertion_is_not_an_integrity_event() {
    let base = "#[test]\nfn five() {\n    let x = 5;\n    assert_eq!(x, 5);\n    \
                let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
    let moved = "#[test]\nfn five() {\n    let x = 5;\n    let a = 1;\n    let b = 2;\n    \
                 let c = 3;\n    assert_eq!(x, 5);\n}\n";
    let repo = Repo::new(&[(TESTS_FILE, base)]);
    repo.write(TESTS_FILE, moved);
    repo.commit("move an assertion");

    // The diff must remove the assertion line for the edit rule to see a move.
    let diff = repo.git(&["diff", "main", "--", TESTS_FILE]);
    assert!(diff.contains("\n-    assert_eq!(x, 5);"), "{diff}");
    assert!(diff.contains("\n+    assert_eq!(x, 5);"), "{diff}");
    assert_eq!(repo.events(&[]), Vec::new());
}

#[test]
fn ratchet_file_change_is_an_integrity_event() {
    let ledger = "loom/maintainability-baseline.txt";
    let repo = Repo::new(&[(ledger, "file src/a.rs 420\n"), ("notes.txt", "same\n")]);
    repo.write(ledger, "file src/a.rs 430\n");

    let ratchet = [ledger.to_string(), "notes.txt".to_string()];
    let expected = IntegrityEvent {
        id: format!("TI-ratchet-{ledger}"),
        kind: EventKind::Ratchet,
        language: None,
        path: Some(ledger.to_string()),
        base: None,
        current: None,
        current_sha256: Some(sha256_hex("file src/a.rs 430\n")),
        detail: Vec::new(),
    };
    assert_eq!(repo.events(&ratchet), vec![expected]);
}

/// `integrity.json` in the D13 shape: `TI-assert-rust` down to `floor`, and
/// the test file's edit at the content hashing to `edit_sha256`.
fn accept(work_dir: &Path, floor: u64, edit_sha256: &str) {
    let record = serde_json::json!({ "version": 1, "accepted": [
        { "event": "TI-assert-rust", "kind": "assert_total", "language": "rust",
          "base": 5, "accepted_current": floor, "dispute": 1 },
        { "event": format!("TI-edit-{TESTS_FILE}"), "kind": "assertion_edit",
          "path": TESTS_FILE, "accepted_sha256": edit_sha256, "dispute": 2 },
    ] });
    let dir = work_dir.join("reviews").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("integrity.json"), record.to_string()).unwrap();
}

#[test]
fn accepted_event_passes_until_it_worsens() {
    let repo = Repo::new(&[(TESTS_FILE, asserting(5).as_str())]);
    let work_dir = repo.tmp.path().join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let stage = Stage {
        id: STAGE.to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    };
    let gate = || check(&stage, &work_dir, &repo.root, "main");

    repo.write(TESTS_FILE, &asserting(4));
    let unaccepted = gate().unwrap_err().to_string();
    assert!(unaccepted.contains("TI-assert-rust (assertions 5 at base, 4 now): not accepted"));

    accept(&work_dir, 4, &sha256_hex(&asserting(4)));
    gate().unwrap();

    // The edit is accepted again at the new content, so only the count blocks.
    repo.write(TESTS_FILE, &asserting(3));
    accept(&work_dir, 4, &sha256_hex(&asserting(3)));
    let worse = gate().unwrap_err().to_string();
    assert!(
        worse.contains("TI-assert-rust (assertions 5 at base, 3 now): worse than accepted"),
        "{worse}"
    );
    assert!(!worse.contains("TI-edit-"), "{worse}");
}
