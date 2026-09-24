//! Bounded project discovery shared by prompt hooks and stage skill routing.

mod markers;
mod probe;
mod runners;
mod scan;
mod scope;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ProjectType {
    pub kind: String,
    /// Package directory, relative to the checkout root.
    pub path: PathBuf,
}

#[derive(Debug, Default, Serialize)]
pub struct ProjectProfile {
    pub root: PathBuf,
    pub types: Vec<ProjectType>,
    /// Includes packages whose stack has no Loom skill, so they still prevent
    /// inheriting an unrelated language from a parent workspace manifest.
    pub packages: Vec<PathBuf>,
    pub truncated: bool,
}

/// One package's detected stack, as `loom project detect` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageDetail {
    /// Package directory, relative to the checkout root.
    pub path: PathBuf,
    pub kinds: Vec<String>,
    /// Test-runner adapter name; `None` when no adapter applies.
    pub runner: Option<&'static str>,
    pub skills: Vec<String>,
}

/// The skill name a detected kind resolves to, without the `loom-` prefix.
/// Plain JavaScript shares the TypeScript skill.
pub fn skill_base(kind: &str) -> &str {
    if kind == "javascript" {
        "typescript"
    } else {
        kind
    }
}

impl ProjectProfile {
    pub fn discover(cwd: &Path) -> Self {
        let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
        let root = scan::checkout_root(&cwd);
        scan::discover(root)
    }

    /// Select the nearest package for each assignment, including packages below
    /// directory/glob assignments. Unrelated sibling packages never contribute.
    pub fn for_files(&self, files: &[String]) -> Vec<ProjectType> {
        scope::for_files(self, files)
    }

    pub fn for_prompt(&self, cwd: &Path, prompt: &str) -> Vec<ProjectType> {
        scope::for_prompt(self, cwd, prompt)
    }

    /// Kinds, test runner and skills of every discovered package.
    pub fn package_details(&self) -> Vec<PackageDetail> {
        self.packages
            .iter()
            .map(|path| {
                let kinds: Vec<String> = self
                    .types
                    .iter()
                    .filter(|kind| kind.path == *path)
                    .map(|kind| kind.kind.clone())
                    .collect();
                let runner = runners::detect_runner(&self.root.join(path), &self.root, &kinds);
                let skills: BTreeSet<String> = kinds
                    .iter()
                    .map(|kind| format!("loom-{}", skill_base(kind)))
                    .collect();
                PackageDetail {
                    path: path.clone(),
                    kinds,
                    runner,
                    skills: skills.into_iter().collect(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
