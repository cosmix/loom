//! Single-directory manifest evidence shared by language and project detection.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_LISTED_ENTRIES: usize = 4_096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LanguageEvidence {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    Kotlin,
    Scala,
    CSharp,
    Ruby,
    Php,
    Swift,
    Elixir,
    Cpp,
    Dart,
}

impl LanguageEvidence {
    pub(crate) fn project_kind(self) -> &'static str {
        use LanguageEvidence::*;
        match self {
            Rust => "rust",
            TypeScript => "typescript",
            JavaScript => "javascript",
            Python => "python",
            Go => "golang",
            Java => "java",
            Kotlin => "kotlin",
            Scala => "scala",
            CSharp => "csharp",
            Ruby => "ruby",
            Php => "php",
            Swift => "swift",
            Elixir => "elixir",
            Cpp => "cpp",
            Dart => "dart",
        }
    }
}

use LanguageEvidence::*;

const MARKERS: &[(LanguageEvidence, &[&str])] = &[
    (Rust, &["Cargo.toml"]),
    (Go, &["go.mod", "go.sum"]),
    (
        Python,
        &[
            "pyproject.toml",
            "setup.py",
            "requirements.txt",
            "Pipfile",
            "poetry.lock",
        ],
    ),
    (TypeScript, &["tsconfig.json"]),
    (Java, &["pom.xml", "build.gradle", "settings.gradle"]),
    (Kotlin, &["build.gradle.kts", "settings.gradle.kts"]),
    (Scala, &["build.sbt"]),
    (Ruby, &["Gemfile"]),
    (Php, &["composer.json"]),
    (Swift, &["Package.swift"]),
    (Elixir, &["mix.exs"]),
    (
        Cpp,
        &[
            "CMakeLists.txt",
            "meson.build",
            "conanfile.txt",
            "conanfile.py",
            "vcpkg.json",
        ],
    ),
    (Dart, &["pubspec.yaml"]),
];

pub(crate) fn detect(dir: &Path) -> BTreeSet<LanguageEvidence> {
    let mut found = BTreeSet::new();
    if !dir
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_dir())
    {
        return found;
    }
    for (language, names) in MARKERS {
        if names.iter().any(|name| regular_file(&dir.join(name))) {
            found.insert(*language);
        }
    }
    add_extensions(dir, &mut found);
    add_package(dir, &mut found);
    found
}

fn add_extensions(dir: &Path, found: &mut BTreeSet<LanguageEvidence>) {
    for file in files_where(dir, |_| true) {
        let ext = file
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        match ext.as_str() {
            "csproj" | "sln" | "slnx" => {
                found.insert(CSharp);
            }
            "gemspec" => {
                found.insert(Ruby);
            }
            _ => {}
        }
    }
}

fn add_package(dir: &Path, found: &mut BTreeSet<LanguageEvidence>) {
    if regular_file(&dir.join("package.json")) {
        if read_json(dir, &dir.join("package.json")).is_some_and(|package| {
            has_dependency(
                &package,
                &["dependencies", "devDependencies", "peerDependencies"],
                "typescript",
            )
        }) {
            found.insert(TypeScript);
        }
        if !found.contains(&TypeScript) {
            found.insert(JavaScript);
        }
    }
}

pub(crate) fn regular_file(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_file())
}

pub(crate) fn read_json(root: &Path, path: &Path) -> Option<Value> {
    let relative = path.strip_prefix(root).ok()?;
    if !regular_file(path) {
        return None;
    }
    let bytes = crate::fs::safe_read::read_bounded(root, relative, MAX_FILE_BYTES).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) fn has_dependency(manifest: &Value, sections: &[&str], name: &str) -> bool {
    sections.iter().any(|section| {
        manifest
            .get(section)
            .and_then(Value::as_object)
            .is_some_and(|deps| deps.contains_key(name))
    })
}

pub(crate) fn files_where(
    dir: &Path,
    matches: impl Fn(&str) -> bool,
) -> impl Iterator<Item = PathBuf> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .take(MAX_LISTED_ENTRIES)
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(move |entry| matches(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
}
