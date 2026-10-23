//! The scaffold (Decisions 1, 2 and 5): derived from the stage base alone,
//! the `mod` declarations and declaration-only module roots that put each
//! contract file into a test target, and the strict diff that holds a
//! contract writer to exactly those edits.

use std::collections::BTreeMap;

use super::chain::{self, Candidate, Declaring, Failure, Link};
use super::targets::{self, Kind, Package};
use super::{contract_path, join, quote, FileSource};
use crate::verify::contracts::collection::{ScaffoldEdit, ScaffoldModule};

/// Changed lines named per file before the rest are only counted.
const REPORTED_LINES: usize = 20;

/// The merged scaffold of every `(file, test)` contract over `base`.
pub(super) fn derive(
    base: FileSource<'_>,
    contracts: &[(&str, &str)],
) -> Result<Vec<ScaffoldEdit>, String> {
    let mut merged = Vec::new();
    for (file, test) in contracts {
        for edit in derive_one(base, file, test)? {
            merge(&mut merged, edit);
        }
    }
    Ok(merged)
}

/// The scaffold of one contract, from the candidate root needing the fewest
/// edits (ties: the lib, then `src/main.rs`, then path order).
fn derive_one(
    base: FileSource<'_>,
    contract_file: &str,
    test: &str,
) -> Result<Vec<ScaffoldEdit>, String> {
    let file = contract_path(contract_file)?;
    let package = targets::package_of(base, &file).ok_or_else(|| targets::no_package(&file))?;
    let (candidates, excluded) = chain::candidates(base, &package, &file, test);
    let mut derived = Vec::new();
    let mut furthest: Option<Failure> = None;
    for candidate in &candidates {
        match chain::walk(base, &package, candidate, &file, false) {
            Ok(links) => derived.push((rank(&package, candidate), edits(links))),
            Err(failure) => failure.keep_furthest(&mut furthest),
        }
    }
    let best = derived
        .into_iter()
        .min_by_key(|(rank, edits)| (edits.len(), rank.clone()));
    match (best, furthest) {
        (Some((_, edits)), _) => Ok(edits),
        (None, Some(failure)) => Err(failure.problems.join("; ")),
        (None, None) => {
            let mut problems = vec![chain::no_target(&package, &file, test)];
            problems.extend(excluded);
            Err(problems.join("; "))
        }
    }
}

/// The tie-break order of a candidate root.
fn rank(package: &Package, candidate: &Candidate) -> (u8, String) {
    let root = &candidate.root;
    let order = if root.kind == Kind::Lib {
        0
    } else if root.path == join(&package.dir, "src/main.rs") {
        1
    } else {
        2
    };
    (order, root.path.clone())
}

/// The edits a lenient walk's links call for; the last link's module is the
/// contract file's own.
fn edits(links: Vec<Link>) -> Vec<ScaffoldEdit> {
    let last = links.len().saturating_sub(1);
    links
        .into_iter()
        .enumerate()
        .filter_map(|(index, link)| {
            let module = ScaffoldModule {
                name: link.module,
                test_only: index == last,
            };
            match link.declaring {
                Declaring::File(_) if link.declared => None,
                Declaring::File(path) => Some(ScaffoldEdit::Declare { path, module }),
                Declaring::New(paths) => Some(ScaffoldEdit::CreateRoot {
                    paths,
                    modules: vec![module],
                }),
            }
        })
        .collect()
}

/// Add `edit` to `merged`: one `Declare` per path and module, one
/// `CreateRoot` per paths with the union of their modules. A module some
/// contract needs outside tests is not test-only.
fn merge(merged: &mut Vec<ScaffoldEdit>, edit: ScaffoldEdit) {
    match edit {
        ScaffoldEdit::Declare { path, module } => {
            let known = merged.iter_mut().find_map(|known| match known {
                ScaffoldEdit::Declare {
                    path: known_path,
                    module: known_module,
                } if *known_path == path && known_module.name == module.name => Some(known_module),
                _ => None,
            });
            match known {
                Some(known) => known.test_only &= module.test_only,
                None => merged.push(ScaffoldEdit::Declare { path, module }),
            }
        }
        ScaffoldEdit::CreateRoot { paths, modules } => {
            let known = merged.iter_mut().find_map(|known| match known {
                ScaffoldEdit::CreateRoot {
                    paths: known_paths,
                    modules: known_modules,
                } if *known_paths == paths => Some(known_modules),
                _ => None,
            });
            match known {
                Some(known) => modules
                    .into_iter()
                    .for_each(|module| add_module(known, module)),
                None => merged.push(ScaffoldEdit::CreateRoot { paths, modules }),
            }
        }
    }
}

fn add_module(modules: &mut Vec<ScaffoldModule>, module: ScaffoldModule) {
    match modules.iter_mut().find(|known| known.name == module.name) {
        Some(known) => known.test_only &= module.test_only,
        None => modules.push(module),
    }
}

/// The strict diff: per `Declare` path, the current file less the derived
/// declarations must be the base file; per `CreateRoot`, exactly one new
/// file holding only them.
pub(super) fn problems(
    edits: &[ScaffoldEdit],
    base: FileSource<'_>,
    current: FileSource<'_>,
) -> Vec<String> {
    let mut declared: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut created = Vec::new();
    for edit in edits {
        match edit {
            ScaffoldEdit::Declare { path, module } => {
                declared.entry(path).or_default().push(&module.name);
            }
            ScaffoldEdit::CreateRoot { paths, modules } => {
                created.extend(created_problems(paths, modules, base, current));
            }
        }
    }
    let mut problems: Vec<String> = declared
        .into_iter()
        .flat_map(|(path, modules)| declared_problems(path, &modules, base, current))
        .collect();
    problems.extend(created);
    problems
}

fn declared_problems(
    path: &str,
    modules: &[&str],
    base: FileSource<'_>,
    current: FileSource<'_>,
) -> Vec<String> {
    let Some(base_text) = base(path) else {
        return vec![format!(
            "{path} is not in the stage base: the scaffold declares modules in existing files only"
        )];
    };
    let Some(current_text) = current(path) else {
        return vec![format!(
            "{path} was removed: a scaffold file keeps every line of the stage base"
        )];
    };
    let lines = non_blank(&current_text);
    let (rest, mut problems) = strip(path, &lines, modules);
    problems.extend(compare(path, &non_blank(&base_text), &rest));
    problems
}

fn created_problems(
    paths: &[String],
    modules: &[ScaffoldModule],
    base: FileSource<'_>,
    current: FileSource<'_>,
) -> Vec<String> {
    let mut problems: Vec<String> = paths
        .iter()
        .filter(|path| base(path.as_str()).is_some())
        .map(|path| format!("{path} exists in the stage base: a scaffold module root must be new"))
        .collect();
    let present: Vec<(&String, String)> = paths
        .iter()
        .filter_map(|path| Some((path, current(path.as_str())?)))
        .collect();
    match present.as_slice() {
        [] => problems.push(format!(
            "none of {} exists: create one holding only {}",
            paths.join(", "),
            declarations(modules)
        )),
        [(path, text)] => {
            let names: Vec<&str> = modules.iter().map(|module| module.name.as_str()).collect();
            // An attribute left above a declaration is itself an extra line.
            let (rest, _) = strip(path, &non_blank(text), &names);
            problems.extend(compare(path, &[], &rest));
        }
        _ => {
            let found: Vec<&str> = present.iter().map(|(path, _)| path.as_str()).collect();
            problems.push(format!(
                "{} all exist: a scaffold module root is exactly one of them",
                found.join(", ")
            ));
        }
    }
    problems
}

/// What a created module root holds, as the writer would write it.
fn declarations(modules: &[ScaffoldModule]) -> String {
    let declarations: Vec<String> = modules
        .iter()
        .map(|module| {
            if module.test_only {
                format!("`#[cfg(test)] mod {};`", module.name)
            } else {
                format!("`mod {};`", module.name)
            }
        })
        .collect();
    declarations.join(" and ")
}

/// The lines of `text` that are not blank after trimming.
fn non_blank(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .collect()
}

/// `lines` without, per module, its first declaration line (`mod m;`,
/// `pub mod m;` or `pub(crate) mod m;`, trimmed) and at most one
/// `#[cfg(test)]` line directly above it; with a problem for each removed
/// declaration that sits under a kept attribute, which would otherwise move
/// from a base item onto it.
fn strip<'t>(path: &str, lines: &[&'t str], modules: &[&str]) -> (Vec<&'t str>, Vec<String>) {
    let mut removed = vec![false; lines.len()];
    let mut blocks = Vec::new();
    for module in modules {
        let Some(at) =
            (0..lines.len()).find(|&at| !removed[at] && is_declaration(lines[at], module))
        else {
            continue;
        };
        removed[at] = true;
        let mut start = at;
        if at > 0 && !removed[at - 1] && lines[at - 1].trim() == "#[cfg(test)]" {
            removed[at - 1] = true;
            start = at - 1;
        }
        blocks.push((start, *module));
    }
    let problems = blocks
        .iter()
        .filter_map(|&(start, module)| {
            let above = (0..start).rev().find(|&index| !removed[index])?;
            let line = lines[above].trim();
            line.starts_with("#[").then(|| {
                format!(
                    "{path}: the declaration of {module} sits under `{}`, which would then apply to it",
                    quote(line)
                )
            })
        })
        .collect();
    let rest = lines
        .iter()
        .zip(&removed)
        .filter(|(_, removed)| !**removed)
        .map(|(line, _)| *line)
        .collect();
    (rest, problems)
}

fn is_declaration(line: &str, module: &str) -> bool {
    let line = line.trim();
    ["mod ", "pub mod ", "pub(crate) mod "]
        .iter()
        .any(|prefix| {
            line.strip_prefix(prefix)
                .and_then(|rest| rest.strip_suffix(';'))
                == Some(module)
        })
}

/// Line-for-line comparison of `current` with `base`: the lines between the
/// common head and tail are extra (current) or missing (base).
fn compare(path: &str, base: &[&str], current: &[&str]) -> Vec<String> {
    let head = base.iter().zip(current).take_while(|(a, b)| a == b).count();
    let tail = base[head..]
        .iter()
        .rev()
        .zip(current[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let extra = &current[head..current.len() - tail];
    let missing = &base[head..base.len() - tail];
    let named = extra
        .iter()
        .map(|line| format!("{path}: extra line `{}`: a scaffold file may only gain the derived mod declarations", quote(line)))
        .chain(missing.iter().map(|line| format!("{path}: line `{}` of the stage base is missing", quote(line))));
    let mut problems: Vec<String> = named.take(REPORTED_LINES).collect();
    let unnamed = (extra.len() + missing.len()).saturating_sub(REPORTED_LINES);
    if unnamed > 0 {
        problems.push(format!("{path}: {unnamed} more changed lines"));
    }
    problems
}

#[cfg(test)]
#[path = "tests_scaffold.rs"]
mod tests;
