//! Tests for the build key of a resolved contract: what two contracts share
//! when they compile the same code.

use super::*;
use crate::testrun::command::HOSTILE_NAME;
use crate::testrun::registry;
use crate::verify::contracts::test_support::contract;
use tempfile::TempDir;

const FIRST: &str = "alpha_one";
const SECOND: &str = "beta_two";

fn spec(adapter: &str, file: &str, test: &str) -> ContractSpec {
    ContractSpec {
        file: file.to_string(),
        test: test.to_string(),
        runner: Some(adapter.to_string()),
        ..contract()
    }
}

fn resolve(
    tmp: &TempDir,
    adapter: &'static dyn TestRunnerAdapter,
    c: &ContractSpec,
) -> ContractRun {
    std::fs::create_dir_all(tmp.path().join(&c.file).parent().unwrap()).unwrap();
    std::fs::write(tmp.path().join(&c.file), "").unwrap();
    resolve_run(c, Some(adapter), tmp.path(), tmp.path(), None).unwrap()
}

fn build_of(
    tmp: &TempDir,
    adapter: &'static dyn TestRunnerAdapter,
    file: &str,
    test: &str,
) -> String {
    let run = resolve(tmp, adapter, &spec(adapter.name(), file, test));
    run.build.expect("an adapter applies")
}

#[test]
fn two_tests_of_one_file_share_a_build_that_names_neither() {
    for adapter in registry::all() {
        let tmp = TempDir::new().unwrap();
        for second in [SECOND, HOSTILE_NAME, "a.b*c[d]\\e\"f"] {
            let first = build_of(&tmp, *adapter, "src/a_test.x", FIRST);
            let other = build_of(&tmp, *adapter, "src/a_test.x", second);
            let name = adapter.name();
            assert_eq!(first, other, "{name}: one file, one build");
            for test in [FIRST, second] {
                assert!(!first.contains(test), "{name}: {first} names {test}");
            }
        }
    }
}

#[test]
fn the_real_command_still_names_its_test() {
    let tmp = TempDir::new().unwrap();
    for adapter in registry::all() {
        let run = resolve(&tmp, *adapter, &spec(adapter.name(), "src/a_test.x", FIRST));
        assert!(
            run.command.contains(FIRST),
            "{}: {}",
            adapter.name(),
            run.command
        );
    }
}

#[test]
fn a_runner_that_names_the_file_builds_once_per_file() {
    let tmp = TempDir::new().unwrap();
    for name in ["jest", "vitest", "pytest", "rspec", "phpunit"] {
        let adapter = registry::by_name(name).unwrap();
        let one = build_of(&tmp, adapter, "src/a_test.x", FIRST);
        let two = build_of(&tmp, adapter, "src/b_test.x", FIRST);
        assert_ne!(one, two, "{name}: two files, two builds");
    }
}

#[test]
fn a_project_runner_builds_once_per_run_directory() {
    let tmp = TempDir::new().unwrap();
    for name in ["gradle", "maven", "sbt"] {
        let adapter = registry::by_name(name).unwrap();
        let one = build_of(&tmp, adapter, "src/a_test.x", FIRST);
        let two = build_of(&tmp, adapter, "src/b_test.x", FIRST);
        assert_eq!(one, two, "{name}: one project, one build");
    }
}

#[test]
fn a_go_package_is_one_build() {
    let tmp = TempDir::new().unwrap();
    let go = registry::by_name("go-test").unwrap();
    let same = |file| build_of(&tmp, go, file, FIRST);
    assert_eq!(same("pkg/a/a_test.go"), same("pkg/a/b_test.go"));
    assert_ne!(same("pkg/a/a_test.go"), same("pkg/b/a_test.go"));
}

#[test]
fn a_contract_without_an_adapter_has_no_build_key() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("t.sh"), "").unwrap();
    let c = ContractSpec {
        file: "t.sh".to_string(),
        test: "./t.sh".to_string(),
        runner: None,
        ..contract()
    };
    let run = resolve_run(&c, None, tmp.path(), tmp.path(), None).unwrap();
    assert_eq!((run.build, run.command.as_str()), (None, "./t.sh"));
}

#[cfg(feature = "source-graph")]
#[test]
fn cargo_builds_once_per_target() {
    let tmp = TempDir::new().unwrap();
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    for (path, content) in [
        ("Cargo.toml", manifest),
        (
            "src/lib.rs",
            "#[cfg(test)]\nmod tests { #[test] fn a() {} }\n",
        ),
        ("tests/x.rs", "#[test]\nfn a() {}\n#[test]\nfn b() {}\n"),
    ] {
        std::fs::create_dir_all(tmp.path().join(path).parent().unwrap()).unwrap();
        std::fs::write(tmp.path().join(path), content).unwrap();
    }
    let cargo = registry::by_name("cargo-test").unwrap();
    let lib = build_of(&tmp, cargo, "src/lib.rs", "tests::a");
    let integration = build_of(&tmp, cargo, "tests/x.rs", "a");
    let integration_b = build_of(&tmp, cargo, "tests/x.rs", "b");
    assert_ne!(lib, integration, "`--lib` and `--test x` are two builds");
    assert_eq!(integration, integration_b);
}

#[test]
fn unscoped_cargo_contracts_share_one_build() {
    let tmp = TempDir::new().unwrap();
    let cargo = registry::by_name("cargo-test").unwrap();
    let one = build_of(&tmp, cargo, "src/a.rs", "a::t1");
    let two = build_of(&tmp, cargo, "src/b.rs", "b::t2");
    assert_eq!(one, two);
}
