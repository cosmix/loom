//! The Cargo target model (Decision 4), read from `Cargo.toml` with the
//! `toml` crate: no `cargo metadata`, which spawns a process and can write
//! `Cargo.lock`.
//!
//! Roots are discovered relative to the contract file: auto roots under
//! `src/bin/` and `tests/` are only those named by the file's own path, so no
//! directory listing is needed. A root the model misses can only make the
//! chain check refuse, never pass.

use toml::{Table, Value};

use super::{clean, contract_path, join, parent, FileSource};

/// The package that owns a file: the nearest ancestor directory whose
/// `Cargo.toml` parses and has a `[package]` table.
pub(super) struct Package {
    /// Its directory, "" for the source root.
    pub dir: String,
    /// `package.name`, else its directory.
    pub name: String,
    manifest: Table,
}

/// The kind of a target, which orders the derivation's tie-break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Lib,
    Bin,
    Test,
}

/// A target root that exists in the source (or is the contract file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Root {
    /// Relative to the source root.
    pub path: String,
    pub kind: Kind,
    /// Why cargo runs no libtest tests from it; `None` for a test target.
    pub not_test: Option<&'static str>,
}

/// The package owning `file`, `None` when no ancestor manifest has a
/// `[package]` table (a workspace-only manifest is passed over).
pub(super) fn package_of(source: FileSource<'_>, file: &str) -> Option<Package> {
    ancestors(parent(file)).find_map(|dir| {
        let manifest = manifest(source, dir)?;
        let package = manifest.get("package")?.as_table()?;
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_else(|| shown(dir))
            .to_string();
        Some(Package {
            dir: dir.to_string(),
            name,
            manifest,
        })
    })
}

/// The problem when no package owns `file`.
pub(super) fn no_package(file: &str) -> String {
    format!("no Cargo package owns {file}")
}

/// The lib, bin and test roots of `package` that may reach `file`, in that
/// order, each present in `source` unless it is `file` itself (a new
/// integration test is its own root).
pub(super) fn test_roots(source: FileSource<'_>, package: &Package, file: &str) -> Vec<Root> {
    let local = package.local(file).unwrap_or_default();
    let present = |relative: &str| {
        let path = join(&package.dir, relative);
        path == file || source(&path).is_some()
    };
    let mut roots: Vec<Root> = package.lib_root().into_iter().collect();
    roots.extend(package.kind_roots(source, Kind::Bin, local, &present));
    roots.extend(package.kind_roots(source, Kind::Test, local, &present));
    roots
        .into_iter()
        .filter(|root| present(&root.path))
        .map(|root| Root {
            path: join(&package.dir, &root.path),
            ..root
        })
        .collect()
}

/// Problems with running `contract_file`'s tests from `run_dir`: cargo picks
/// the manifest nearest the directory it runs in, and from a workspace root
/// above the package it compiles `default-members` or every member, so a
/// same-named test in another member could stand in for the frozen one.
pub(super) fn run_dir_problems(
    source: FileSource<'_>,
    contract_file: &str,
    run_dir: &str,
) -> Vec<String> {
    let file = match contract_path(contract_file) {
        Ok(file) => file,
        Err(problem) => return vec![problem],
    };
    let Some(package) = package_of(source, &file) else {
        return vec![no_package(&file)];
    };
    let Some(run) = clean(run_dir) else {
        return vec![format!(
            "run directory {run_dir} is not a plain relative path"
        )];
    };
    let want = shown(&package.dir);
    let nearest = ancestors(&run).find(|dir| source(&join(dir, "Cargo.toml")).is_some());
    match nearest {
        Some(dir) if dir == package.dir && package.selects_itself() => Vec::new(),
        Some(dir) => vec![format!(
            "cargo runs contract tests in {}, whose manifest {} does not select package {want}; \
             set the contract's cwd to {want}",
            shown(&run),
            join(dir, "Cargo.toml")
        )],
        None => vec![format!(
            "cargo runs contract tests in {}, which has no Cargo.toml at or above it; \
             set the contract's cwd to {want}",
            shown(&run)
        )],
    }
}

impl Package {
    /// `file` relative to the package directory.
    pub(super) fn local<'f>(&self, file: &'f str) -> Option<&'f str> {
        if self.dir.is_empty() {
            return Some(file);
        }
        file.strip_prefix(self.dir.as_str())?.strip_prefix('/')
    }

    /// A key of the `[package]` table.
    fn key(&self, name: &str) -> Option<&Value> {
        self.manifest.get("package")?.as_table()?.get(name)
    }

    /// Whether the `auto*` flag `name` leaves auto-discovery on.
    fn auto(&self, name: &str) -> bool {
        self.key(name).and_then(Value::as_bool) != Some(false)
    }

    /// `[lib] path`, else `src/lib.rs` unless `autolib = false`.
    fn lib_root(&self) -> Option<Root> {
        let table = self.manifest.get("lib").and_then(Value::as_table);
        let path = match table
            .and_then(|lib| lib.get("path"))
            .and_then(Value::as_str)
        {
            Some(path) => clean(path)?,
            None if self.auto("autolib") => "src/lib.rs".to_string(),
            None => return None,
        };
        Some(Root {
            path,
            kind: Kind::Lib,
            not_test: not_test(table),
        })
    }

    /// The `[[bin]]` or `[[test]]` entries, then the auto roots they do not
    /// replace; paths relative to the package.
    fn kind_roots(
        &self,
        source: FileSource<'_>,
        kind: Kind,
        local: &str,
        present: &dyn Fn(&str) -> bool,
    ) -> Vec<Root> {
        let key = if kind == Kind::Bin { "bin" } else { "test" };
        let entries: Vec<&Table> = self
            .manifest
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_table)
            .collect();
        let mut roots: Vec<Root> = entries
            .iter()
            .filter_map(|entry| {
                Some(Root {
                    path: self.entry_path(kind, entry, present)?,
                    kind,
                    not_test: not_test(Some(entry)),
                })
            })
            .collect();
        if !entries.is_empty() && self.edition_2015(source) {
            return roots;
        }
        for path in self.auto_paths(kind, local) {
            if !roots.iter().any(|root| root.path == path) {
                roots.push(Root {
                    path,
                    kind,
                    not_test: None,
                });
            }
        }
        roots
    }

    /// An entry's `path`, else the first present default for its `name`.
    fn entry_path(
        &self,
        kind: Kind,
        entry: &Table,
        present: &dyn Fn(&str) -> bool,
    ) -> Option<String> {
        if let Some(path) = entry.get("path").and_then(Value::as_str) {
            return clean(path);
        }
        let name = entry.get("name").and_then(Value::as_str)?;
        let defaults = match kind {
            Kind::Bin if name == self.name => vec!["src/main.rs".to_string()],
            Kind::Bin => vec![
                format!("src/bin/{name}.rs"),
                format!("src/bin/{name}/main.rs"),
            ],
            _ => vec![format!("tests/{name}.rs"), format!("tests/{name}/main.rs")],
        };
        defaults
            .iter()
            .filter_map(|path| clean(path))
            .find(|path| present(path))
    }

    /// The auto roots of `kind` that may reach the package-relative `local`.
    fn auto_paths(&self, kind: Kind, local: &str) -> Vec<String> {
        let (flag, dir) = match kind {
            Kind::Bin => ("autobins", "src/bin"),
            _ => ("autotests", "tests"),
        };
        if !self.auto(flag) {
            return Vec::new();
        }
        let mut paths = Vec::new();
        if kind == Kind::Bin {
            paths.push("src/main.rs".to_string());
        }
        if let Some(rest) = local
            .strip_prefix(dir)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            let name = match rest.split_once('/') {
                Some((name, _)) => name,
                None => rest.strip_suffix(".rs").unwrap_or(rest),
            };
            paths.push(format!("{dir}/{name}.rs"));
            paths.push(format!("{dir}/{name}/main.rs"));
        }
        paths
    }

    /// Edition 2015, where declaring one target of a kind turns that kind's
    /// auto-discovery off. A manifest without `edition` is 2015, and so is
    /// one whose inherited edition cannot be read (fewer roots: fail closed).
    fn edition_2015(&self, source: FileSource<'_>) -> bool {
        match self.key("edition") {
            Some(Value::String(edition)) => edition == "2015",
            Some(Value::Table(inherit))
                if inherit.get("workspace") == Some(&Value::Boolean(true)) =>
            {
                workspace_edition(source, &self.dir)
                    .as_deref()
                    .unwrap_or("2015")
                    == "2015"
            }
            _ => true,
        }
    }

    /// Whether cargo run in the package directory tests the package: not
    /// when its manifest is also a workspace root whose `default-members`
    /// leave the root package out.
    fn selects_itself(&self) -> bool {
        let members = self
            .manifest
            .get("workspace")
            .and_then(Value::as_table)
            .and_then(|workspace| workspace.get("default-members"))
            .and_then(Value::as_array);
        members.is_none_or(|members| {
            members
                .iter()
                .filter_map(Value::as_str)
                .any(|member| clean(member).as_deref() == Some(""))
        })
    }
}

/// Why the target `entry` runs no libtest tests: cargo skips a target with
/// `test = false`, runs a `harness = false` one without libtest, and skips
/// one with `required-features` silently unless those features are on.
fn not_test(entry: Option<&Table>) -> Option<&'static str> {
    let entry = entry?;
    let off = |key: &str| entry.get(key).and_then(Value::as_bool) == Some(false);
    let features = entry
        .get("required-features")
        .and_then(Value::as_array)
        .is_some_and(|features| !features.is_empty());
    if off("test") {
        Some("test = false")
    } else if off("harness") {
        Some("harness = false")
    } else if features {
        Some("required-features")
    } else {
        None
    }
}

/// `workspace.package.edition` of the nearest workspace manifest at or
/// above `dir`.
fn workspace_edition(source: FileSource<'_>, dir: &str) -> Option<String> {
    let workspace = ancestors(dir).find_map(|dir| {
        let mut manifest = manifest(source, dir)?;
        match manifest.remove("workspace")? {
            Value::Table(workspace) => Some(workspace),
            _ => None,
        }
    })?;
    let edition = workspace.get("package")?.as_table()?.get("edition")?;
    edition.as_str().map(str::to_string)
}

/// The parsed `Cargo.toml` in `dir`, `None` when absent or unparsable.
fn manifest(source: FileSource<'_>, dir: &str) -> Option<Table> {
    toml::from_str(&source(&join(dir, "Cargo.toml"))?).ok()
}

/// `dir`, then each directory above it, ending with the source root "".
fn ancestors(dir: &str) -> impl Iterator<Item = &str> {
    std::iter::successors(Some(dir), |dir| (!dir.is_empty()).then(|| parent(dir)))
}

/// A directory for a message: "." for the source root.
fn shown(dir: &str) -> &str {
    if dir.is_empty() {
        "."
    } else {
        dir
    }
}

#[cfg(test)]
#[path = "tests_targets.rs"]
mod tests;
