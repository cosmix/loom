use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::detect_runner;
use crate::skills::project::{PackageDetail, ProjectProfile};
use crate::skills::{recommend, SkillIndex};

type Files = &'static [(&'static str, &'static str)];

/// One package per D7 row and variant: its marker files, then the expected adapter.
const ECOSYSTEMS: &[(Files, Option<&str>)] = &[
    (&[("Cargo.toml", "[package]")], Some("cargo-test")),
    (
        &[("Cargo.toml", "[package]"), (".config/nextest.toml", "")],
        Some("cargo-nextest"),
    ),
    (&[("go.mod", "module example")], Some("go-test")),
    (
        &[("pyproject.toml", "[project]"), ("conftest.py", "")],
        Some("pytest"),
    ),
    (&[("setup.py", ""), ("pytest.ini", "")], Some("pytest")),
    (
        &[("pyproject.toml", "[tool.pytest.ini_options]\n")],
        Some("pytest"),
    ),
    (
        &[("setup.py", ""), ("requirements-dev.txt", "pytest==8\n")],
        Some("pytest"),
    ),
    (&[("pyproject.toml", "[project]")], Some("unittest")),
    (
        &[("pyproject.toml", "[tool.pytest]\nminversion = \"9.0\"\n")],
        Some("pytest"),
    ),
    (
        &[(
            "pyproject.toml",
            "[project]\ndependencies = [\"pytest>=8\"]\n",
        )],
        Some("pytest"),
    ),
    (
        &[(
            "pyproject.toml",
            "[project]\nname = \"notpytest-fixtures\"\n",
        )],
        Some("unittest"),
    ),
    (
        &[("pyproject.toml", "[project]\n# not using pytest\n")],
        Some("unittest"),
    ),
    (
        &[("package.json", r#"{"devDependencies":{"jest":"30"}}"#)],
        Some("jest"),
    ),
    (
        &[
            ("package.json", r#"{"dependencies":{"mocha":"11"}}"#),
            ("tsconfig.json", "{}"),
        ],
        Some("mocha"),
    ),
    (
        &[("package.json", "{}"), ("bun.lockb", "")],
        Some("bun-test"),
    ),
    (
        &[("package.json", r#"{"scripts":{"test":"node --test"}}"#)],
        Some("node-test"),
    ),
    (&[("package.json", "{}")], None),
    (&[("build.gradle", "")], Some("gradle")),
    (&[("build.gradle.kts", "")], Some("gradle")),
    (&[("settings.gradle", "")], Some("gradle")),
    // The JavaScript row names no runner, so the next matching row decides.
    (
        &[("package.json", "{}"), ("build.gradle", "")],
        Some("gradle"),
    ),
    (&[("pom.xml", "<project/>")], Some("maven")),
    (&[("build.sbt", "")], Some("sbt")),
    (&[("App.csproj", "<Project/>")], Some("dotnet-test")),
    (&[("App.sln", "")], Some("dotnet-test")),
    (&[("Gemfile", "gem 'rspec'\n")], Some("rspec")),
    (
        &[("Gemfile", "gem 'minitest'\n"), (".rspec", "")],
        Some("rspec"),
    ),
    (&[("Gemfile", "gem 'minitest'\n")], Some("minitest")),
    (
        &[("Gemfile", "group :test do\n  gem \"rspec-rails\"\nend\n")],
        Some("rspec"),
    ),
    (
        &[("Gemfile", "# gem \"rspec\"\ngem 'minitest'\n")],
        Some("minitest"),
    ),
    (
        &[("composer.json", r#"{"require-dev":{"pestphp/pest":"^3"}}"#)],
        Some("pest"),
    ),
    (
        &[(
            "composer.json",
            r#"{"require-dev":{"phpunit/phpunit":"^11"}}"#,
        )],
        Some("phpunit"),
    ),
    (&[("Package.swift", "")], Some("swift-test")),
    (&[("mix.exs", "")], Some("mix-test")),
    (&[("CMakeLists.txt", "")], Some("ctest")),
    (
        &[(
            "pubspec.yaml",
            "dependencies:\n  flutter:\n    sdk: flutter\n",
        )],
        Some("flutter-test"),
    ),
    (&[("pubspec.yaml", "name: app\n")], Some("dart-test")),
    (
        &[("pubspec.yaml", "name: app\n# sdk: flutter\n")],
        Some("dart-test"),
    ),
    (&[("Dockerfile", "FROM scratch")], None),
];

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A real `.git` keeps discovery inside the temp directory.
fn checkout() -> TempDir {
    let repo = TempDir::new().unwrap();
    write(repo.path(), ".git/HEAD", "ref: refs/heads/main\n");
    repo
}

fn detail(details: &[PackageDetail], path: &str) -> PackageDetail {
    details
        .iter()
        .find(|detail| detail.path == Path::new(path))
        .cloned()
        .unwrap_or_else(|| panic!("no package {path} in {details:?}"))
}

#[test]
fn detects_runner_for_each_ecosystem() {
    let repo = checkout();
    for (index, (files, _)) in ECOSYSTEMS.iter().enumerate() {
        for (name, content) in *files {
            write(repo.path(), &format!("p{index}/{name}"), content);
        }
    }
    let details = ProjectProfile::discover(repo.path()).package_details();
    for (index, (files, expected)) in ECOSYSTEMS.iter().enumerate() {
        let package = detail(&details, &format!("p{index}"));
        assert_eq!(
            package.runner, *expected,
            "{files:?} kinds={:?}",
            package.kinds
        );
    }
    // Nextest configured once at the checkout root applies to every Rust package.
    write(repo.path(), ".config/nextest.toml", "");
    let rust = ["rust".to_string()];
    let root = repo.path();
    assert_eq!(
        detect_runner(&root.join("p0"), root, &rust),
        Some("cargo-nextest")
    );
}

#[test]
fn javascript_package_maps_to_typescript_skill() {
    let repo = checkout();
    write(repo.path(), "tools/package.json", r#"{"name":"plain-js"}"#);
    write(repo.path(), "web/package.json", "{}");
    write(repo.path(), "web/tsconfig.json", "{}");
    let details = ProjectProfile::discover(repo.path()).package_details();
    let tools = detail(&details, "tools");
    assert_eq!(tools.kinds, ["javascript"]);
    assert_eq!(tools.skills, ["loom-typescript"]);
    assert_eq!(detail(&details, "web").kinds, ["typescript"]);

    let skills = TempDir::new().unwrap();
    write(
        skills.path(),
        "loom-typescript/SKILL.md",
        "---\nname: loom-typescript\ndescription: Test skill\ntriggers: []\n---\n",
    );
    let index = SkillIndex::load_from_directory(skills.path()).unwrap();
    let matches = recommend::for_files(
        &index,
        "adjust the script",
        repo.path(),
        &["tools/index.js".into()],
        &[],
    );
    let names: Vec<&str> = matches.iter().map(|skill| skill.name.as_str()).collect();
    assert_eq!(names, ["loom-typescript"]);
}

/// D7 calls any `package.json` without a `tsconfig.json` JavaScript; a
/// `typescript` dependency is TypeScript evidence too, and the kinds never co-occur.
#[test]
fn typescript_dependency_without_tsconfig_is_not_javascript() {
    let repo = checkout();
    write(
        repo.path(),
        "app/package.json",
        r#"{"devDependencies":{"typescript":"5"}}"#,
    );
    let details = ProjectProfile::discover(repo.path()).package_details();
    let kinds = detail(&details, "app").kinds;
    assert!(kinds.contains(&"typescript".to_string()), "{kinds:?}");
    assert!(!kinds.contains(&"javascript".to_string()), "{kinds:?}");
}

/// Reads resolve beneath the checkout root without following a symlink at any
/// component, so a package reached through a symlinked directory has no manifest.
#[test]
fn symlinked_package_directory_reads_as_absent() {
    let repo = checkout();
    let root = repo.path();
    write(
        root,
        "real/package.json",
        r#"{"devDependencies":{"jest":"30"}}"#,
    );
    write(root, "real/pyproject.toml", "dependencies = [\"pytest\"]\n");
    std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();
    let javascript = ["javascript".to_string()];
    let python = ["python".to_string()];
    for (kinds, real, linked) in [
        (&javascript, Some("jest"), None),
        (&python, Some("pytest"), Some("unittest")),
    ] {
        assert_eq!(detect_runner(&root.join("real"), root, kinds), real);
        assert_eq!(detect_runner(&root.join("link"), root, kinds), linked);
    }
}

#[test]
fn vitest_dependency_wins_over_bun_lockfile() {
    let repo = checkout();
    write(
        repo.path(),
        "web/package.json",
        r#"{"devDependencies":{"vitest":"4"}}"#,
    );
    write(repo.path(), "web/bun.lock", "{}");
    let details = ProjectProfile::discover(repo.path()).package_details();
    assert_eq!(detail(&details, "web").runner, Some("vitest"));
}
