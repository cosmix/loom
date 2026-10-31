use std::collections::BTreeSet;
use std::path::Path;

use super::probe;
use crate::language::evidence;

const FILE_MARKERS: &[(&str, &[&str])] = &[
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
    let mut types: BTreeSet<String> = evidence::detect(dir)
        .into_iter()
        .map(|language| language.project_kind().to_string())
        .collect();
    for (kind, files) in FILE_MARKERS {
        if any_file(dir, files) {
            types.insert((*kind).to_string());
        }
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
    types
}

/// `scan` passes no checkout root, so the read is anchored at `dir`: a
/// symlinked `dir` or manifest reads as absent; ancestors of `dir` go unchecked.
fn detect_dependencies(dir: &Path, types: &mut BTreeSet<String>) {
    let Some(package) = probe::read_json(dir, &dir.join("package.json")) else {
        return;
    };
    let sections = ["dependencies", "devDependencies", "peerDependencies"];
    let depends_on = |name: &str| probe::has_dependency(&package, &sections, name);
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
    evidence::regular_file(path)
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
