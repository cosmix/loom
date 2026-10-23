use super::harness_source_matches;

#[test]
fn harness_source_matches_flags_only_production_source() {
    let harness = [
        "src/main.rs".to_string(),
        "tests/**".to_string(),
        "src/**/*_tests.rs".to_string(),
    ];
    let files = [
        "src/main.rs".to_string(),
        "tests/fixtures/a.json".to_string(),
        "src/device_lock/contract_tests.rs".to_string(),
        "src/lib.rs".to_string(),
    ];
    assert_eq!(
        harness_source_matches(&harness, &files),
        vec!["src/main.rs"]
    );
}

#[cfg(not(feature = "source-graph"))]
#[test]
fn collection_fails_closed_without_source_graph() {
    use super::rust::{chain_problems, derive_scaffold};
    const MESSAGE: &str =
        "cannot verify Rust contract collection: loom was built without the source-graph feature";
    let source = |_: &str| -> Option<String> { None };
    assert_eq!(chain_problems(&source, "tests/x.rs", "x"), vec![MESSAGE]);
    assert_eq!(
        derive_scaffold(&source, &[("tests/x.rs", "x")]),
        Err(MESSAGE.to_string())
    );
}

#[cfg(feature = "source-graph")]
mod stage {
    use std::collections::BTreeSet;
    use std::path::Path;
    use std::process::Command;

    use tempfile::TempDir;

    use super::super::{freeze_scaffold, stage_scaffold, ScaffoldCheck, ScaffoldEdit};
    use crate::git::worktree::WorktreeGit;
    use crate::plan::schema::ContractSpec;

    const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    const FILE: &str = "src/device_lock/contract_tests.rs";
    const DECLARED_MAIN: &str = "mod device_lock;\nmod devices;\n\nfn main() {}\n";

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .current_dir(root)
            .env("GIT_CONFIG_GLOBAL", root.join(".no-global-config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }

    fn write(root: &Path, path: &str, content: &str) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// A repository whose package lives in `pkg/`, the stage working
    /// directory, committed in the liquidmon shape, with the contract file
    /// written and untracked.
    fn repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        git(root, &["init", "-q", "-b", "main"]);
        write(root, "pkg/Cargo.toml", MANIFEST);
        write(root, "pkg/src/main.rs", "mod devices;\n\nfn main() {}\n");
        write(root, "pkg/src/devices.rs", "");
        git(root, &["add", "pkg"]);
        git(root, &["commit", "-q", "-m", "base"]);
        write(root, &format!("pkg/{FILE}"), "#[test]\nfn rejects_x() {}\n");
        temp
    }

    fn contract(runner: &str) -> ContractSpec {
        ContractSpec {
            id: "rejects-x".to_string(),
            file: FILE.to_string(),
            test: "device_lock::contract_tests::rejects_x".to_string(),
            runner: Some(runner.to_string()),
            cwd: None,
            scenario: "s".to_string(),
            rejects: "r".to_string(),
        }
    }

    fn freeze(root: &Path, contracts: &[ContractSpec]) -> ScaffoldCheck {
        let repo = WorktreeGit::discovered(root);
        freeze_scaffold(&repo, root, &root.join("pkg"), "HEAD", contracts).unwrap()
    }

    #[test]
    fn the_scaffold_is_relative_to_the_working_directory() {
        let temp = repo();
        let root = temp.path();
        let repo = WorktreeGit::discovered(root);
        let edits = stage_scaffold(
            &repo,
            root,
            &root.join("pkg"),
            "HEAD",
            &[contract("cargo-test")],
        )
        .unwrap();
        assert!(
            matches!(&edits[0], ScaffoldEdit::Declare { path, .. } if path == "src/main.rs"),
            "{edits:?}"
        );
    }

    #[test]
    fn a_clean_scaffold_and_chain_pass_the_freeze() {
        let temp = repo();
        let root = temp.path();
        write(root, "pkg/src/main.rs", DECLARED_MAIN);
        write(
            root,
            "pkg/src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        );
        let check = freeze(root, &[contract("cargo-test")]);
        assert!(check.problems.is_empty(), "{:?}", check.problems);
        let expected: BTreeSet<String> = [
            "src/main.rs",
            "src/device_lock/mod.rs",
            "src/device_lock.rs",
        ]
        .map(String::from)
        .into();
        assert_eq!(check.paths, expected);
    }

    #[test]
    fn production_code_in_a_scaffold_root_is_refused_before_the_chain() {
        let temp = repo();
        let root = temp.path();
        write(root, "pkg/src/main.rs", DECLARED_MAIN);
        write(
            root,
            "pkg/src/device_lock/mod.rs",
            "mod contract_tests;\npub fn lock() {}\n",
        );
        let check = freeze(root, &[contract("cargo-test")]);
        assert_eq!(check.problems.len(), 1, "{:?}", check.problems);
        assert!(check.problems[0].contains("extra line `pub fn lock() {}`"));
    }

    #[test]
    fn a_missing_declaration_fails_the_chain_check() {
        let temp = repo();
        let root = temp.path();
        write(
            root,
            "pkg/src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        );
        let check = freeze(root, &[contract("cargo-test")]);
        assert_eq!(
            check.problems,
            vec!["contract 'rejects-x': pkg/src/main.rs does not declare device_lock: add `mod device_lock;`"]
        );
    }

    #[test]
    fn a_symlinked_scaffold_root_counts_as_absent() {
        let temp = repo();
        let root = temp.path();
        write(root, "pkg/src/main.rs", DECLARED_MAIN);
        write(
            root,
            "pkg/elsewhere.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        );
        std::os::unix::fs::symlink(
            root.join("pkg/elsewhere.rs"),
            root.join("pkg/src/device_lock/mod.rs"),
        )
        .unwrap();
        let check = freeze(root, &[contract("cargo-test")]);
        assert!(
            check
                .problems
                .iter()
                .any(|problem| problem.starts_with("none of src/device_lock/mod.rs")),
            "{:?}",
            check.problems
        );
    }

    #[test]
    fn a_cwd_at_a_workspace_root_above_the_package_is_refused() {
        let temp = repo();
        let root = temp.path();
        write(
            root,
            "Cargo.toml",
            "[workspace]\nmembers = [\"pkg\"]\ndefault-members = []\n",
        );
        write(root, "pkg/src/main.rs", DECLARED_MAIN);
        write(
            root,
            "pkg/src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        );
        let mut spec = contract("cargo-test");
        spec.cwd = Some(".".to_string());
        let check = freeze(root, &[spec]);
        assert_eq!(check.problems.len(), 1, "{:?}", check.problems);
        assert!(
            check.problems[0].starts_with("contract 'rejects-x': cargo runs contract tests in .")
        );
    }

    #[test]
    fn a_stage_without_rust_contracts_has_no_scaffold() {
        let temp = repo();
        let root = temp.path();
        let repo = WorktreeGit::discovered(root);
        let check = freeze_scaffold(
            &repo,
            root,
            &root.join("pkg"),
            "not-a-revision",
            &[contract("pytest")],
        )
        .unwrap();
        assert!(check.paths.is_empty() && check.problems.is_empty());
    }

    #[test]
    fn an_unresolvable_base_is_an_error() {
        let temp = repo();
        let root = temp.path();
        let repo = WorktreeGit::discovered(root);
        for base in ["no-such-revision", "--output=x"] {
            assert!(freeze_scaffold(
                &repo,
                root,
                &root.join("pkg"),
                base,
                &[contract("cargo-test")]
            )
            .is_err());
        }
    }
}
