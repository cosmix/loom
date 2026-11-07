//! The writer's choices: parsing the flags, merging and pruning them, the
//! daemon's `choices.json`, the overlays, and every rule `validate_choices`
//! enforces, against real repositories with a committed `pkg/Cargo.toml`.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::*;

const CARGO_TOML: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n";

/// A repository on `main` in a temporary directory, its git isolated from
/// the user's and the system's configuration.
struct Repo {
    _tmp: TempDir,
    root: PathBuf,
}

impl Repo {
    fn init() -> Self {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let repo = Self { _tmp: tmp, root };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "user.email", "t@t"]);
        repo.git(&["config", "user.name", "t"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.root)
            .env("GIT_CONFIG_GLOBAL", self.root.join(".no-global"))
            .env("GIT_CONFIG_SYSTEM", self.root.join(".no-system"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    fn write(&self, path: &str, body: &str) {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// Commit every file and return the commit.
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&[
            "commit",
            "-q",
            "--no-gpg-sign",
            "--allow-empty",
            "-m",
            message,
        ]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }
}

/// A repository whose base holds `pkg/Cargo.toml` and the contract file
/// `pkg/tests/a.rs`, with the stage's working directory at the root.
fn cargo_repo() -> (Repo, String) {
    let repo = Repo::init();
    repo.write("pkg/Cargo.toml", CARGO_TOML);
    repo.write("pkg/tests/a.rs", "#[test]\nfn t() {}\n");
    let base = repo.commit("base");
    (repo, base)
}

fn spec(id: &str, file: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: file.to_string(),
        test: None,
        runner: None,
        cwd: None,
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

fn choice(test: Option<&str>, runner: Option<&str>, cwd: Option<&str>) -> ContractChoice {
    ContractChoice {
        test: test.map(str::to_string),
        runner: runner.map(str::to_string),
        cwd: cwd.map(str::to_string),
    }
}

fn one(id: &str, choice: ContractChoice) -> ContractChoices {
    ContractChoices::from([(id.to_string(), choice)])
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

/// `validate_choices` over `cargo_repo`'s contract with the plan spec `plan`.
fn check(plan: ContractSpec, choices: &ContractChoices) -> Result<()> {
    let (repo, base) = cargo_repo();
    validate_choices(&[plan], choices, &repo.root, &repo.root, &base)
}

fn refusal(plan: ContractSpec, choices: &ContractChoices) -> String {
    format!(
        "{:#}",
        check(plan, choices).expect_err("the choices are refused")
    )
}

fn open() -> ContractSpec {
    spec("c1", "pkg/tests/a.rs")
}

#[test]
fn parse_splits_at_the_first_equals_and_trims() {
    let parsed = parse_choices(&strings(&["a= x=y "]), &strings(&["b=cargo-test"]), &[]).unwrap();
    assert_eq!(parsed["a"], choice(Some("x=y"), None, None));
    assert_eq!(parsed["b"], choice(None, Some("cargo-test"), None));
    assert!(parse_choices(&[], &[], &[]).unwrap().is_empty());
}

#[test]
fn parse_refuses_malformed_and_oversized_entries() {
    let duplicate = parse_choices(&[], &[], &strings(&["a=pkg", "a=web"])).unwrap_err();
    assert!(format!("{duplicate:#}").contains("--cwd"), "{duplicate:#}");
    let long_runner = vec![format!("a={}", "r".repeat(65))];
    assert!(parse_choices(&[], &long_runner, &[]).is_err());
    let long_cwd = vec![format!("a={}", "d".repeat(513))];
    assert!(parse_choices(&[], &[], &long_cwd).is_err());
    assert!(parse_choices(&strings(&["a=x\ty"]), &[], &[]).is_err());
    assert!(parse_choices(&strings(&["a=   "]), &[], &[]).is_err());
    let many: Vec<String> = (0..65).map(|i| format!("c{i}=pkg")).collect();
    assert!(parse_choices(&[], &[], &many).is_err());
    let bulky: Vec<String> = (0..33)
        .map(|i| format!("c{i}={}", "t".repeat(500)))
        .collect();
    assert!(
        parse_choices(&bulky, &[], &[]).is_err(),
        "16500 bytes of values"
    );
    assert!(
        parse_choices(&bulky[..32], &[], &[]).is_ok(),
        "16000 bytes of values"
    );
}

#[test]
fn merge_keeps_what_the_update_leaves_unset() {
    let current = one("a", choice(Some("t1"), Some("cargo-test"), Some("pkg")));
    let update = one("a", choice(Some("t2"), None, None));
    let merged = merge_choices(&current, &update);
    assert_eq!(
        merged["a"],
        choice(Some("t2"), Some("cargo-test"), Some("pkg"))
    );
}

#[test]
fn choices_round_trip_and_a_malformed_file_names_the_stage() {
    let work = TempDir::new().unwrap();
    assert!(load_choices(work.path(), "s1").unwrap().is_empty());
    let saved = one("a", choice(Some("x"), None, Some("pkg")));
    save_choices(work.path(), "s1", &saved).unwrap();
    assert_eq!(load_choices(work.path(), "s1").unwrap(), saved);
    std::fs::write(work.path().join("contracts/s1/choices.json"), "{not json").unwrap();
    let error = load_choices(work.path(), "s1").unwrap_err();
    assert!(format!("{error:#}").contains("'s1'"), "{error:#}");
    assert!(
        load_choices(work.path(), "../s1").is_err(),
        "the stage id is validated"
    );
}

#[test]
fn prune_drops_stale_ids_and_fields_the_plan_now_sets() {
    let plan = [ContractSpec {
        runner: Some("vitest".to_string()),
        ..spec("a", "web/a.test.ts")
    }];
    let mut saved = one("a", choice(Some("suite > case"), Some("jest"), Some("web")));
    saved.insert("gone".to_string(), choice(Some("x"), None, None));
    let pruned = prune_choices(&plan, &saved);
    assert_eq!(
        pruned,
        one("a", choice(Some("suite > case"), None, Some("web")))
    );
    let only_runner = one("a", choice(None, Some("jest"), None));
    assert!(
        prune_choices(&plan, &only_runner).is_empty(),
        "an empty choice is dropped"
    );
}

#[test]
fn overlays_read_the_saved_choices() {
    let work = TempDir::new().unwrap();
    let plan = [spec("a", "pkg/tests/a.rs"), spec("b", "pkg/tests/b.rs")];
    save_choices(
        work.path(),
        "s1",
        &one("a", choice(Some("t"), None, Some("pkg"))),
    )
    .unwrap();
    let update = one("b", choice(None, Some("cargo-test"), None));
    let overlaid = overlay_with_saved(work.path(), "s1", &plan, &update).unwrap();
    assert_eq!(overlaid[0].test.as_deref(), Some("t"));
    assert_eq!(overlaid[0].cwd.as_deref(), Some("pkg"));
    assert_eq!(overlaid[1].runner.as_deref(), Some("cargo-test"));
    assert_eq!(
        overlaid[1].test, None,
        "no hint and no saved test stays open"
    );

    let stage = Stage {
        id: "s1".to_string(),
        contracts: plan.to_vec(),
        ..Stage::default()
    };
    let error = effective_contracts(work.path(), &stage).unwrap_err();
    assert!(format!("{error:#}").contains("contract 'b'"), "{error:#}");
    assert_eq!(
        lenient_contracts(work.path(), &stage)[0].test.as_deref(),
        Some("t")
    );
    std::fs::write(work.path().join("contracts/s1/choices.json"), "[]").unwrap();
    assert_eq!(lenient_contracts(work.path(), &stage), plan.to_vec());
}

#[test]
fn a_choice_for_no_contract_is_refused() {
    let plan = ContractSpec {
        test: Some("t".to_string()),
        ..open()
    };
    let message = refusal(plan, &one("c9", choice(Some("t"), None, None)));
    assert!(message.contains("'c9'"), "{message}");
}

#[test]
fn every_contract_needs_a_test() {
    let message = refusal(open(), &ContractChoices::new());
    assert!(message.contains("--test c1=<identifier>"), "{message}");
    let plan = ContractSpec {
        runner: Some("cargo-test".to_string()),
        ..open()
    };
    check(plan, &one("c1", choice(Some("t"), None, None))).expect("the writer names it");
}

#[test]
fn a_writer_test_is_an_identifier() {
    let plan = || ContractSpec {
        runner: Some("cargo-test".to_string()),
        ..open()
    };
    let long = "t".repeat(513);
    for test in [
        "--help",
        "-Zunstable-options",
        " -x",
        "",
        "  ",
        "a\nb",
        long.as_str(),
    ] {
        let refused = check(plan(), &one("c1", choice(Some(test), None, None)));
        assert!(refused.is_err(), "test {test:?} must be refused");
    }
    let hinted = ContractSpec {
        test: Some("echo a\nexit 1".to_string()),
        ..open()
    };
    check(hinted, &ContractChoices::new()).expect("the plan's own hint is the planner's to give");
}

#[test]
fn the_plan_runner_and_cwd_win_over_the_writer() {
    let plan = ContractSpec {
        test: Some("t".to_string()),
        runner: Some("cargo-test".to_string()),
        cwd: Some("pkg".to_string()),
        ..open()
    };
    let runner = refusal(
        plan.clone(),
        &one("c1", choice(None, Some("cargo-test"), None)),
    );
    assert!(
        runner.contains("the plan sets runner 'cargo-test'"),
        "{runner}"
    );
    let cwd = refusal(plan, &one("c1", choice(None, None, Some("pkg"))));
    assert!(cwd.contains("the plan sets cwd 'pkg'"), "{cwd}");
}

#[test]
fn an_unknown_runner_and_a_shell_test_are_refused() {
    let plan = ContractSpec {
        test: Some("t".to_string()),
        ..open()
    };
    let unknown = refusal(plan, &one("c1", choice(None, Some("no-such"), None)));
    assert!(unknown.contains("'no-such'"), "{unknown}");
    let repo = Repo::init();
    repo.write("checks/a.sh", "exit 1\n");
    let base = repo.commit("base");
    let shell = [spec("c1", "checks/a.sh")];
    let chosen = one("c1", choice(Some("plain_name"), None, None));
    let verdict = validate_choices(&shell, &chosen, &repo.root, &repo.root, &base);
    assert!(format!("{:#}", verdict.unwrap_err()).contains("shell command"));
}

fn cwd_check(repo: &Repo, base: &str, working: &Path, file: &str, cwd: &str) -> Result<()> {
    let plan = [ContractSpec {
        test: Some("t".to_string()),
        runner: Some("cargo-test".to_string()),
        ..spec("c1", file)
    }];
    let choices = one("c1", choice(None, None, Some(cwd)));
    validate_choices(&plan, &choices, &repo.root, working, base)
}

#[test]
fn a_writer_cwd_stays_inside_the_working_dir_above_the_file() {
    let (repo, base) = cargo_repo();
    let root = repo.root.clone();
    let pkg = root.join("pkg");
    let absolute = pkg.display().to_string();
    for refused in [
        "..",
        "pkg/..",
        absolute.as_str(),
        "pkg/tests",
        "pkg/tests/a.rs",
        "src",
    ] {
        let verdict = cwd_check(&repo, &base, &root, "pkg/tests/a.rs", refused);
        assert!(verdict.is_err(), "cwd {refused:?} must be refused");
    }
    let outside = cwd_check(&repo, &base, &pkg, "tests/a.rs", ".").unwrap_err();
    assert!(format!("{outside:#}").contains("outside the stage's working directory"));
    cwd_check(&repo, &base, &pkg, "tests/a.rs", "pkg").expect("cwd equal to working_dir");
    cwd_check(&repo, &base, &root, "pkg/tests/a.rs", "pkg").expect("a cwd beneath it");
}

#[test]
fn a_writer_cwd_needs_its_marker_committed_at_base() {
    let (repo, base) = cargo_repo();
    let root = repo.root.clone();
    repo.write("pkg/tests/Cargo.toml", CARGO_TOML);
    repo.write("pkg/tests/inner/a.rs", "#[test]\nfn t() {}\n");
    let untracked = cwd_check(&repo, &base, &root, "pkg/tests/inner/a.rs", "pkg/tests");
    assert!(format!("{:#}", untracked.unwrap_err()).contains("committed at the stage base"));
    repo.commit("marker after base");
    let late = cwd_check(&repo, &base, &root, "pkg/tests/inner/a.rs", "pkg/tests");
    assert!(late.is_err(), "a marker committed after base is refused");

    let linked = Repo::init();
    linked.write("real.toml", CARGO_TOML);
    linked.write("pkg/tests/a.rs", "#[test]\nfn t() {}\n");
    std::os::unix::fs::symlink("../real.toml", linked.root.join("pkg/Cargo.toml")).unwrap();
    let base = linked.commit("base");
    let symlink = cwd_check(&linked, &base, &linked.root, "pkg/tests/a.rs", "pkg");
    assert!(symlink.is_err(), "a marker tracked as a symlink is refused");
}

#[test]
fn a_writer_cwd_needs_an_adapter_with_markers() {
    let (repo, base) = cargo_repo();
    let plan = [ContractSpec {
        test: Some("test_a".to_string()),
        runner: Some("pytest".to_string()),
        ..spec("c1", "pkg/tests/a.rs")
    }];
    let choices = one("c1", choice(None, None, Some("pkg")));
    let verdict = validate_choices(&plan, &choices, &repo.root, &repo.root, &base);
    assert!(format!("{:#}", verdict.unwrap_err()).contains("pytest has none"));
}

#[test]
fn an_extension_marker_matches_by_extension() {
    assert!(validate::matches_marker(
        "App.csproj",
        &["*.sln", "*.csproj"]
    ));
    assert!(!validate::matches_marker("App.csproj.bak", &["*.csproj"]));
    assert!(validate::matches_marker("Cargo.toml", &["Cargo.toml"]));
    assert!(!validate::matches_marker("cargo.toml", &["Cargo.toml"]));
}
