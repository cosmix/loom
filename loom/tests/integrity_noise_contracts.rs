//! Contracts: the test-integrity detector compares assertion statements, so a
//! reflow beside a real edit raises nothing and a changed continuation line
//! raises; a test moved into another file cancels only when the runner still
//! collects it and it runs identically; a cancelling partner joins the
//! accepted pin; a ratchet file raises only when it loosens.

use loom::verify::integrity::{
    current_events, shortfall, Accepted, AcceptedEvent, EventKind, IntegrityEvent,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

const TESTS: &str = "src/calc_tests.rs";
const MORE: &str = "src/calc_more_tests.rs";
const EDIT_ID: &str = "TI-edit-src/calc_tests.rs";
const CARGO: &str = "[package]\nname = \"calc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LIB: &str =
    "pub fn compute(a: i32, b: i32) -> i32 {\n    a + b\n}\n\n#[cfg(test)]\nmod calc_tests;\n";
const MORE_DECL: &str = "\n#[cfg(test)]\nmod calc_more_tests;\n";
const ADDS: &str = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n}\n";
const SUBTRACTS: &str =
    "#[test]\nfn subtracts() {\n    let a = 1;\n    assert_eq!(compute(a, -1), 0);\n}\n";
const DOUBLES: &str = "#[test]\nfn doubles() {\n    assert_eq!(compute(2, 2), 4);\n}\n";
const LEDGER: &str = "file src/a.rs 430\nfunction src/b.rs run 60\n";
const THRESHOLDS: &str = "coverage 80%\n";
const RATCHETS: [&str; 2] = ["ledger.txt", "thresholds.txt"];

/// A calc crate committed on `main`, with branch `feature` checked out.
struct Repo {
    _temp: TempDir,
    root: PathBuf,
}

impl Repo {
    /// Cargo.toml, src/lib.rs, `tests` as src/calc_tests.rs and `extra` on main.
    fn new(tests: &str, extra: &[(&str, &str)]) -> Self {
        let temp = TempDir::new().expect("create repo dir");
        let root = temp.path().canonicalize().expect("canonical repo dir");
        let repo = Self { _temp: temp, root };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.write("Cargo.toml", CARGO);
        repo.write("src/lib.rs", LIB);
        repo.write(TESTS, tests);
        for (path, content) in extra {
            repo.write(path, content);
        }
        repo.commit("base");
        repo.git(&["switch", "-q", "-c", "feature"]);
        repo
    }

    /// Git isolated from the host's global and system configuration.
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_CONFIG_GLOBAL", self.root.join(".loom-test-no-global"))
            .env("GIT_CONFIG_SYSTEM", self.root.join(".loom-test-no-system"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write(&self, path: &str, content: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().expect("parent dir")).expect("create dirs");
        fs::write(path, content).expect("write file");
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).unwrap_or_default()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
        ]);
    }

    /// Commits the worktree, then returns its events against `main`.
    fn events(&self, ratchet_files: &[&str]) -> Vec<IntegrityEvent> {
        self.commit("feature work");
        let ratchet: Vec<String> = ratchet_files.iter().map(|p| p.to_string()).collect();
        current_events(&self.root, "main", &ratchet).expect("current_events")
    }
}

/// A Rust test file under src/ holding `fns`.
fn tests_file(fns: &[&str]) -> String {
    format!("use crate::compute;\n\n{}", fns.join("\n"))
}

fn ids(events: &[IntegrityEvent]) -> Vec<&str> {
    events.iter().map(|event| event.id.as_str()).collect()
}

fn edit_ids(events: &[IntegrityEvent]) -> Vec<&str> {
    events
        .iter()
        .filter(|event| event.kind == EventKind::AssertionEdit)
        .map(|event| event.id.as_str())
        .collect()
}

fn event(events: &[IntegrityEvent], id: &str) -> IntegrityEvent {
    events
        .iter()
        .find(|event| event.id == id)
        .cloned()
        .unwrap_or_else(|| panic!("no {id} in {:?}", ids(events)))
}

/// A repo whose base src/calc_tests.rs holds adds and subtracts, plus `extra`.
fn move_repo(extra: &[(&str, &str)]) -> Repo {
    Repo::new(&tests_file(&[ADDS, SUBTRACTS]), extra)
}

/// Leaves only adds in src/calc_tests.rs, appends `body` to `dest` and
/// `decl` to src/lib.rs, and returns the events.
fn move_subtracts(repo: &Repo, dest: &str, body: &str, decl: &str) -> Vec<IntegrityEvent> {
    repo.write(TESTS, &tests_file(&[ADDS]));
    let existing = repo.read(dest);
    let dest_text = if existing.is_empty() {
        tests_file(&[body])
    } else {
        format!("{existing}\n{body}")
    };
    repo.write(dest, &dest_text);
    repo.write("src/lib.rs", &format!("{LIB}{decl}"));
    repo.events(&[])
}

#[test]
fn reflow_beside_a_real_edit_is_no_event() {
    let base = "#[test]\nfn adds() {\n    let a = 1;\n    assert_eq!(compute(1, 2), 3);\n}\n";
    let repo = Repo::new(&tests_file(&[base]), &[]);
    let reflowed = "#[test]\nfn adds() {\n    let a = 2;\n    assert_eq!(\n        compute(1, 2),\n        3,\n    );\n}\n";
    repo.write(TESTS, &tests_file(&[reflowed]));

    let events = repo.events(&[]);

    assert!(
        events.is_empty(),
        "a rewrapped assert beside an edited line raised {events:?}"
    );
}

#[test]
fn changed_continuation_line_is_an_event() {
    let base =
        "#[test]\nfn adds() {\n    assert_eq!(\n        compute(1, 2),\n        3,\n    );\n}\n";
    let repo = Repo::new(&tests_file(&[base]), &[]);
    repo.write(
        TESTS,
        &tests_file(&[&base.replace("        3,\n", "        4,\n")]),
    );

    let events = repo.events(&[]);

    assert_eq!(ids(&events), vec![EDIT_ID], "events={events:?}");
    let edit = event(&events, EDIT_ID);
    assert!(
        edit.detail
            .iter()
            .any(|statement| statement.contains("compute(1, 2), 3,") && !statement.contains("4,")),
        "detail does not name the base statement: {:?}",
        edit.detail
    );
}

#[test]
fn moved_test_cancels_only_when_it_still_runs_identically() {
    let verbatim = move_subtracts(&move_repo(&[]), MORE, SUBTRACTS, MORE_DECL);
    assert!(
        verbatim.is_empty(),
        "A: a verbatim move into a collected file raised {verbatim:?}"
    );

    let changed_body = SUBTRACTS.replace("let a = 1;", "let a = 2;");
    let changed = move_subtracts(&move_repo(&[]), MORE, &changed_body, MORE_DECL);
    assert_eq!(
        edit_ids(&changed),
        vec![EDIT_ID],
        "B: a move whose body changed beside an identical assert cancelled: {changed:?}"
    );

    let ignored = format!("#[ignore]\n{SUBTRACTS}");
    let disabled = move_subtracts(&move_repo(&[]), MORE, &ignored, MORE_DECL);
    assert_eq!(
        edit_ids(&disabled),
        vec![EDIT_ID],
        "C: a move that gained #[ignore] cancelled: {disabled:?}"
    );
}

#[test]
fn moved_test_cancels_only_into_a_collected_file() {
    let declared = move_subtracts(&move_repo(&[]), MORE, SUBTRACTS, MORE_DECL);
    assert!(
        declared.is_empty(),
        "A: a move into a declared new file raised {declared:?}"
    );

    let undeclared = move_subtracts(&move_repo(&[]), MORE, SUBTRACTS, "");
    assert_eq!(
        edit_ids(&undeclared),
        vec![EDIT_ID],
        "B: a move into a file no mod line reaches cancelled: {undeclared:?}"
    );

    let spare = "#[test]\nfn spare() {\n    assert_eq!(compute(0, 0), 0);\n}\n";
    let unused = tests_file(&[spare]);
    let repo = move_repo(&[("src/unused_tests.rs", unused.as_str())]);
    let into_unused = move_subtracts(&repo, "src/unused_tests.rs", SUBTRACTS, "");
    assert_eq!(
        edit_ids(&into_unused),
        vec![EDIT_ID],
        "C: a move into an undeclared base test file cancelled: {into_unused:?}"
    );

    let repo = move_repo(&[]);
    repo.write(
        "src/calc.rs",
        "#[cfg(test)]\n#[path = \"split_tests.rs\"]\nmod split;\n",
    );
    let via_path = move_subtracts(&repo, "src/split_tests.rs", SUBTRACTS, "\nmod calc;\n");
    assert!(
        via_path.is_empty(),
        "D: a move into a file declared through #[path] raised {via_path:?}"
    );
}

#[test]
fn edited_move_partner_invalidates_the_accepted_pin() {
    let repo = Repo::new(&tests_file(&[ADDS, SUBTRACTS, DOUBLES]), &[]);
    let first = event(&move_subtracts(&repo, MORE, SUBTRACTS, MORE_DECL), EDIT_ID);
    let accepted = Accepted {
        accepted: vec![AcceptedEvent {
            event: first.id.clone(),
            kind: EventKind::AssertionEdit,
            language: first.language.clone(),
            path: first.path.clone(),
            base: None,
            accepted_current: None,
            accepted_sha256: first.current_sha256.clone(),
            dispute: 1,
        }],
        ..Accepted::default()
    };
    assert_eq!(shortfall(&first, &accepted), None, "event={first:?}");

    let edited = SUBTRACTS.replace("let a = 1;", "let a = 3;");
    repo.write(MORE, &tests_file(&[&edited]));
    let again = event(&repo.events(&[]), EDIT_ID);

    assert!(
        shortfall(&again, &accepted).is_some(),
        "the moved test's partner changed but the accepted pin still covers {again:?}"
    );
}

/// A repo whose base holds both ratchet files, with `file` set to `content`.
fn ratchet_ids(file: &str, content: &str) -> Vec<String> {
    let repo = Repo::new(
        &tests_file(&[ADDS]),
        &[("ledger.txt", LEDGER), ("thresholds.txt", THRESHOLDS)],
    );
    repo.write(file, content);
    ids(&repo.events(&RATCHETS))
        .into_iter()
        .map(str::to_string)
        .collect()
}

#[test]
fn ratchet_raises_only_when_the_ledger_loosens() {
    let tightened = ratchet_ids("ledger.txt", "file src/a.rs 420\n");
    assert!(
        tightened.is_empty(),
        "A: a tightened ledger raised {tightened:?}"
    );

    let raised = ratchet_ids(
        "ledger.txt",
        "file src/a.rs 440\nfunction src/b.rs run 60\n",
    );
    assert_eq!(raised, vec!["TI-ratchet-ledger.txt"], "B: a raised value");

    let added = ratchet_ids("ledger.txt", &format!("{LEDGER}file src/c.rs 500\n"));
    assert_eq!(added, vec!["TI-ratchet-ledger.txt"], "C: a new key");

    let floor = ratchet_ids("thresholds.txt", "coverage 79%\n");
    assert_eq!(
        floor,
        vec!["TI-ratchet-thresholds.txt"],
        "D: an unparsable floor file changed"
    );
}
