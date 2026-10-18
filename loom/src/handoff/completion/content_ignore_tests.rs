//! Committed `.gitignore` rules, in any language layout, decide generated output.
use super::committed_content;
use super::tests::{commit, fixture, git, ignore, write};
use crate::models::stage::Stage;
use std::path::Path;

const RULES: &str = "_build/\ndeps/\nbuild/\n__pycache__/\n";
const OUTPUTS: [&str; 5] = [
    "packages/x/_build/dev/lib/x/ebin/x.beam",
    "packages/x/deps/jason/mix.exs",
    "packages/x/build/classes/java/main/Main.class",
    "packages/contracts/python/src/pkg/__pycache__/x.cpython-314.pyc",
    "packages/native/build/CMakeFiles/app.o",
];

/// A checkout whose committed sources sit beside generated output in an
/// Elixir, Java, Python and C++ layout under `packages/`.
fn polyglot() -> (tempfile::TempDir, Stage, std::path::PathBuf) {
    let (repo, stage, checkout) = fixture();
    for source in [
        "packages/x/mix.exs",
        "packages/contracts/python/src/pkg/x.py",
        "packages/native/CMakeLists.txt",
    ] {
        write(&checkout, source);
    }
    std::fs::write(checkout.join(".gitignore"), RULES).unwrap();
    git(&checkout, &["add", ".gitignore", "packages"]);
    commit(&checkout, "sources");
    for output in OUTPUTS {
        write(&checkout, output);
    }
    (repo, stage, checkout)
}

fn refusal(stage: &Stage, repo: &Path) -> String {
    format!("{:#}", committed_content(stage, repo).unwrap_err())
}

#[test]
fn ignored_output_under_broad_declarations_is_accepted() {
    for pattern in ["packages/**", "packages", "packages/*", "**", "packages/x"] {
        let (repo, mut stage, _checkout) = polyglot();
        stage.files.push(pattern.into());
        stage
            .artifacts
            .push("packages/contracts/python/src/pkg".into());
        committed_content(&stage, repo.path())
            .unwrap_or_else(|error| panic!("{pattern}: {error:#}"));
    }
}

#[test]
fn unignored_source_under_the_same_declaration_is_refused() {
    let (repo, mut stage, checkout) = polyglot();
    stage.files.push("packages/**".into());
    write(&checkout, "packages/x/lib/new.ex");
    assert!(
        refusal(&stage, repo.path()).contains("untracked completion input 'packages/x/lib/new.ex'")
    );
}

#[test]
fn local_exclude_files_grant_no_exemption() {
    let (repo, stage, checkout) = fixture();
    write(&checkout, "out/app.o");
    std::fs::write(repo.path().join(".git/info/exclude"), "out/\n").unwrap();
    assert!(refusal(&stage, repo.path()).contains("'out/app.o'"));
    std::fs::write(repo.path().join(".git/info/exclude"), "").unwrap();
    let excludes = repo.path().join("global-excludes");
    std::fs::write(&excludes, "out/\n").unwrap();
    git(
        repo.path(),
        &["config", "core.excludesFile", excludes.to_str().unwrap()],
    );
    assert!(refusal(&stage, repo.path()).contains("'out/app.o'"));
}

#[test]
fn named_ignored_inputs_are_refused_until_committed() {
    for pattern in [
        "packages/x/_build/dev/lib/x/ebin/x.beam",
        "packages/x/_build",
        "packages/**/_build/**",
        "packages/x/_build/**/*.beam",
        "x/_build",
    ] {
        let (repo, mut stage, checkout) = polyglot();
        stage.working_dir = Some("packages".into());
        stage.harness.push(pattern.into());
        assert!(
            refusal(&stage, repo.path()).contains("declares this ignored output"),
            "{pattern}"
        );
        git(&checkout, &["add", "-f", "packages/x/_build"]);
        commit(&checkout, "required build input");
        committed_content(&stage, repo.path())
            .unwrap_or_else(|error| panic!("{pattern}: {error:#}"));
    }
}

#[test]
fn untracked_ignore_files_grant_no_exemption() {
    let (repo, stage, checkout) = fixture();
    std::fs::create_dir_all(checkout.join("gen")).unwrap();
    std::fs::write(checkout.join("gen/.gitignore"), "*\n").unwrap();
    write(&checkout, "gen/required.rs");
    assert!(refusal(&stage, repo.path()).contains("'gen/.gitignore'"));
}

#[test]
fn ignore_files_inside_ignored_directories_are_output() {
    let (repo, stage, checkout) = fixture();
    ignore(&checkout, ".ruff_cache/\n");
    std::fs::create_dir_all(checkout.join(".ruff_cache/0.14")).unwrap();
    std::fs::write(checkout.join(".ruff_cache/.gitignore"), "*\n").unwrap();
    write(&checkout, ".ruff_cache/0.14/cache");
    committed_content(&stage, repo.path()).unwrap();
}

#[test]
fn uncommitted_ignore_rules_are_refused() {
    let (repo, stage, checkout) = fixture();
    ignore(&checkout, "_build/\n");
    std::fs::write(checkout.join(".gitignore"), "_build/\nsrc/\n").unwrap();
    write(&checkout, "src/required.ex");
    assert!(refusal(&stage, repo.path()).contains("worktree differs"));
}
