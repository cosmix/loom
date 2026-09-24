//! Test-runner selection per package. The rules are DESIGN D7's table; the
//! names returned are the `testrun` adapter names.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::markers::any_file;
use super::probe;

/// A `[tool.pytest]` or `[tool.pytest.ini_options]` table header, or `pytest`
/// as a dependency word: `"pytest>=8"`, `pytest==8` and `pytest-cov` match;
/// `notpytest-fixtures` and `.pytest_cache` do not.
static PYTEST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^\s*\[tool\.pytest(?:\.ini_options)?\]|(?:^|[^A-Za-z0-9_.-])pytest(?:[^A-Za-z0-9_]|$)",
    )
    .expect("Invalid regex")
});
/// A `gem 'rspec'` or `gem "rspec-rails"` declaration.
static RSPEC_GEM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"^\s*gem\s+['"]rspec"#).expect("Invalid regex"));
/// The `sdk: flutter` line of a `flutter:` dependency; `flutter_*` packages
/// and prose do not match.
static FLUTTER_SDK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*sdk:\s*flutter\b").expect("Invalid regex"));

/// `(package_dir, checkout_root)`; files are read beneath the checkout root.
type Rule = fn(&Path, &Path) -> Option<&'static str>;

/// In table order: the first row that matches one of the package's kinds and
/// names a runner wins. Infrastructure kinds match no row.
const RULES: &[(&[&str], Rule)] = &[
    (&["rust"], rust),
    (&["golang"], |_, _| Some("go-test")),
    (&["python"], python),
    (&["typescript", "javascript"], javascript),
    (&["java", "kotlin"], jvm),
    (&["scala"], |_, _| Some("sbt")),
    (&["csharp"], |_, _| Some("dotnet-test")),
    (&["ruby"], ruby),
    (&["php"], php),
    (&["swift"], |_, _| Some("swift-test")),
    (&["elixir"], |_, _| Some("mix-test")),
    (&["cpp"], |_, _| Some("ctest")),
    (&["dart"], dart),
];

/// The adapter that runs `package_dir`'s tests, or `None` when no adapter applies.
/// `package_dir` lies beneath `checkout_root`; a file reached through a symlink
/// anywhere below the checkout root reads as absent.
pub fn detect_runner(
    package_dir: &Path,
    checkout_root: &Path,
    kinds: &[String],
) -> Option<&'static str> {
    RULES
        .iter()
        .filter(|(rule_kinds, _)| kinds.iter().any(|kind| rule_kinds.contains(&kind.as_str())))
        .find_map(|(_, rule)| rule(package_dir, checkout_root))
}

fn rust(package_dir: &Path, checkout_root: &Path) -> Option<&'static str> {
    let nextest = [package_dir, checkout_root]
        .iter()
        .any(|dir| any_file(dir, &[".config/nextest.toml"]));
    Some(if nextest {
        "cargo-nextest"
    } else {
        "cargo-test"
    })
}

/// `PYTEST` covers both a pyproject's table header and its dependency arrays;
/// only its dependency alternative is meaningful in a requirements file.
fn python(dir: &Path, root: &Path) -> Option<&'static str> {
    let requirements = |name: &str| name.starts_with("requirements") && name.ends_with(".txt");
    let pytest = any_file(dir, &["pytest.ini", "conftest.py"])
        || probe::has_line(root, &dir.join("pyproject.toml"), &PYTEST)
        || probe::files_where(dir, requirements).any(|path| probe::has_line(root, &path, &PYTEST));
    Some(if pytest { "pytest" } else { "unittest" })
}

fn javascript(dir: &Path, root: &Path) -> Option<&'static str> {
    let package = probe::read_json(root, &dir.join("package.json")).unwrap_or(Value::Null);
    let sections = ["dependencies", "devDependencies"];
    let declared = ["vitest", "jest", "mocha"]
        .into_iter()
        .find(|runner| probe::has_dependency(&package, &sections, runner));
    if declared.is_some() {
        return declared;
    }
    let test_script = package
        .pointer("/scripts/test")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim_start();
    if any_file(dir, &["bun.lock", "bun.lockb"]) || test_script.starts_with("bun test") {
        return Some("bun-test");
    }
    test_script.contains("node --test").then_some("node-test")
}

/// A `settings.gradle*` without a root build file is still a Gradle multi-project
/// build, so Maven is chosen only when no Gradle file exists at all.
fn jvm(dir: &Path, _: &Path) -> Option<&'static str> {
    let gradle = any_file(
        dir,
        &[
            "build.gradle",
            "build.gradle.kts",
            "settings.gradle",
            "settings.gradle.kts",
        ],
    );
    Some(if gradle { "gradle" } else { "maven" })
}

fn ruby(dir: &Path, root: &Path) -> Option<&'static str> {
    let rspec =
        any_file(dir, &[".rspec"]) || probe::has_line(root, &dir.join("Gemfile"), &RSPEC_GEM);
    Some(if rspec { "rspec" } else { "minitest" })
}

fn php(dir: &Path, root: &Path) -> Option<&'static str> {
    let pest = probe::read_json(root, &dir.join("composer.json")).is_some_and(|composer| {
        probe::has_dependency(&composer, &["require", "require-dev"], "pestphp/pest")
    });
    Some(if pest { "pest" } else { "phpunit" })
}

/// Flutter packages depend on the SDK as `flutter: sdk: flutter`.
fn dart(dir: &Path, root: &Path) -> Option<&'static str> {
    let flutter = probe::has_line(root, &dir.join("pubspec.yaml"), &FLUTTER_SDK);
    Some(if flutter { "flutter-test" } else { "dart-test" })
}

#[cfg(test)]
#[path = "runners_tests.rs"]
mod tests;
