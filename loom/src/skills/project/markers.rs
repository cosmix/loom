use std::collections::BTreeSet;
use std::path::Path;

use super::probe;

const FILE_MARKERS: &[(&str, &[&str])] = &[
    ("rust", &["Cargo.toml"]),
    ("golang", &["go.mod", "go.sum"]),
    (
        "python",
        &[
            "pyproject.toml",
            "setup.py",
            "requirements.txt",
            "Pipfile",
            "poetry.lock",
        ],
    ),
    ("typescript", &["tsconfig.json"]),
    ("java", &["pom.xml", "build.gradle", "settings.gradle"]),
    ("kotlin", &["build.gradle.kts", "settings.gradle.kts"]),
    ("scala", &["build.sbt"]),
    ("ruby", &["Gemfile"]),
    ("php", &["composer.json"]),
    ("swift", &["Package.swift"]),
    ("elixir", &["mix.exs"]),
    ("cpp", &["CMakeLists.txt"]),
    ("dart", &["pubspec.yaml"]),
    (
        "react",
        &[
            "next.config.js",
            "next.config.ts",
            "next.config.mjs",
            "next.config.cjs",
            "remix.config.js",
        ],
    ),
    (
        "docker",
        &[
            "Dockerfile",
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yaml",
            "compose.yml",
        ],
    ),
    ("kustomize", &["kustomization.yaml", "kustomization.yml"]),
    (
        "kubernetes",
        &[
            "kustomization.yaml",
            "kustomization.yml",
            "Chart.yaml",
            "helmfile.yaml",
            "helmfile.yml",
            "skaffold.yaml",
        ],
    ),
    (
        "terraform",
        &[
            "main.tf",
            "terraform.tf",
            ".terraform.lock.hcl",
            "versions.tf",
        ],
    ),
    (
        "ci-cd",
        &[
            ".gitlab-ci.yml",
            ".circleci/config.yml",
            "azure-pipelines.yml",
            "Jenkinsfile",
        ],
    ),
    ("fluxcd", &["flux-system.yaml"]),
    ("prometheus", &["prometheus.yml", "prometheus.yaml"]),
    ("grafana", &["grafana.ini"]),
];

pub(super) fn detect(dir: &Path) -> BTreeSet<String> {
    let mut types = BTreeSet::new();
    for (kind, files) in FILE_MARKERS {
        if any_file(dir, files) {
            types.insert((*kind).to_string());
        }
    }
    if dotnet_project(dir) {
        types.insert("csharp".to_string());
    }
    for (kind, names) in [
        ("ci-cd", &[".github/workflows"][..]),
        ("argocd", &["argocd", ".argocd"][..]),
        ("fluxcd", &["flux", ".flux"][..]),
        ("grafana", &["grafana"][..]),
    ] {
        if names.iter().any(|name| plain_directory(&dir.join(name))) {
            types.insert(kind.to_string());
        }
    }
    detect_dependencies(dir, &mut types);
    // A `package.json` without TypeScript evidence (a `tsconfig.json` or a
    // `typescript` dependency) is plain JavaScript; the two kinds never co-occur.
    if regular_file(&dir.join("package.json")) && !types.contains("typescript") {
        types.insert("javascript".to_string());
    }
    types
}

/// .NET project and solution names vary, so they are matched by extension.
fn dotnet_project(dir: &Path) -> bool {
    probe::files_where(dir, |name| {
        name.ends_with(".csproj") || name.ends_with(".sln")
    })
    .next()
    .is_some()
}

/// `scan` passes no checkout root, so the read is anchored at `dir`: a
/// symlinked `dir` or manifest reads as absent; ancestors of `dir` go unchecked.
fn detect_dependencies(dir: &Path, types: &mut BTreeSet<String>) {
    let Some(package) = probe::read_json(dir, &dir.join("package.json")) else {
        return;
    };
    let sections = ["dependencies", "devDependencies", "peerDependencies"];
    let depends_on = |name: &str| probe::has_dependency(&package, &sections, name);
    if depends_on("typescript") {
        types.insert("typescript".into());
    }
    if [
        "react",
        "react-dom",
        "next",
        "remix",
        "@remix-run/react",
        "@remix-run/node",
    ]
    .into_iter()
    .any(depends_on)
    {
        types.insert("react".into());
    }
}

pub(super) fn any_file(dir: &Path, names: &[&str]) -> bool {
    names.iter().any(|name| regular_file(&dir.join(name)))
}

pub(super) fn regular_file(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_file())
}

pub(super) fn plain_directory(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_dir())
}

pub(super) fn package_boundary(path: &Path) -> bool {
    any_file(
        path,
        &[
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "tsconfig.json",
        ],
    )
}
