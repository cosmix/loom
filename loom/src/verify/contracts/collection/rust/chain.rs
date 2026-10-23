//! The `mod` chain from a test target root to a contract file (Decisions 3
//! and 5): module paths, the candidate roots, and the walk that checks every
//! link. Completion runs a contract by test name only, so this chain is what
//! proves the frozen file is the one compiled under that name.

use super::items::{self, Attribute, ModItem, ParsedFile};
use super::targets::{self, Package, Root};
use super::{contract_path, join, parent, quote, FileSource};

/// A libtest test target root whose module path for the contract file is a
/// prefix of the contract's test path.
pub(super) struct Candidate {
    pub root: Root,
    /// The contract file's module path under the root; empty when the file
    /// is the root.
    pub modules: Vec<String>,
}

/// The file that declares one module of a chain.
pub(super) enum Declaring {
    /// A file that exists.
    File(String),
    /// A module root that does not exist yet, offered at these paths.
    New(Vec<String>),
}

/// One link of a chain: `module` declared (or not yet) in `declaring`.
pub(super) struct Link {
    pub module: String,
    pub declaring: Declaring,
    pub declared: bool,
}

/// A chain that broke after `links` clean links.
pub(super) struct Failure {
    pub links: usize,
    pub problems: Vec<String>,
}

impl Failure {
    /// Keep in `furthest` whichever chain got further, the earlier on a tie.
    pub(super) fn keep_furthest(self, furthest: &mut Option<Failure>) {
        if furthest.as_ref().is_none_or(|best| self.links > best.links) {
            *furthest = Some(self);
        }
    }
}

/// The chain check: empty when some candidate root reaches `contract_file`
/// through clean links, else the problems of the chain that got furthest.
pub(super) fn problems(source: FileSource<'_>, contract_file: &str, test: &str) -> Vec<String> {
    let file = match contract_path(contract_file) {
        Ok(file) => file,
        Err(problem) => return vec![problem],
    };
    let Some(package) = targets::package_of(source, &file) else {
        return vec![targets::no_package(&file)];
    };
    let (candidates, excluded) = candidates(source, &package, &file, test);
    let mut furthest: Option<Failure> = None;
    for candidate in &candidates {
        match walk(source, &package, candidate, &file, true) {
            Ok(_) => return Vec::new(),
            Err(failure) => failure.keep_furthest(&mut furthest),
        }
    }
    match furthest {
        Some(failure) => failure.problems,
        None => std::iter::once(no_target(&package, &file, test))
            .chain(excluded)
            .collect(),
    }
}

/// The problem when no candidate root exists.
pub(super) fn no_target(package: &Package, file: &str, test: &str) -> String {
    format!(
        "no test target of package {} compiles {file} as {test}",
        package.name
    )
}

/// The candidate roots for `file` as `test`, and why each other root that
/// reaches the file is none.
pub(super) fn candidates(
    source: FileSource<'_>,
    package: &Package,
    file: &str,
    test: &str,
) -> (Vec<Candidate>, Vec<String>) {
    let mut found = Vec::new();
    let mut excluded = Vec::new();
    for root in targets::test_roots(source, package, file) {
        let Some(modules) = module_path(&root.path, file) else {
            continue;
        };
        if let Some(reason) = root.not_test {
            excluded.push(format!(
                "{} reaches {file} but is not a libtest test target ({reason})",
                root.path
            ));
        } else if !prefixes(&modules, test) {
            excluded.push(format!(
                "the test path {test} does not start with {}, the module path of {file} in {}",
                shown_path(&modules),
                root.path
            ));
        } else {
            found.push(Candidate { root, modules });
        }
    }
    (found, excluded)
}

/// The module path of `file` under the target root `root`: `None` when the
/// file is not beneath the root's directory or a component is no plain
/// identifier (only `#[path]` could reach it). A root's children live in its
/// own directory, as do a `mod.rs` file's; any other `x.rs` keeps them in
/// `x/`, so the path follows the directories. A first module whose file
/// would be the root itself (`tests/it/a.rs` under the root `tests/it.rs`)
/// has no path: rustc looks for the root's children in `tests/`.
pub(super) fn module_path(root: &str, file: &str) -> Option<Vec<String>> {
    if root == file {
        return Some(Vec::new());
    }
    let dir = parent(root);
    let relative = if dir.is_empty() {
        file
    } else {
        file.strip_prefix(dir)?.strip_prefix('/')?
    };
    let mut parts: Vec<&str> = relative.split('/').collect();
    let last = parts.pop()?;
    if last != "mod.rs" {
        parts.push(last.strip_suffix(".rs")?);
    }
    // `<dir>/<first>/mod.rs` cannot be the root, whose directory is `dir`.
    let valid = !parts.is_empty()
        && parts.iter().all(|part| is_identifier(part))
        && join(dir, &format!("{}.rs", parts[0])) != root;
    valid.then(|| parts.iter().map(|part| part.to_string()).collect())
}

/// Walk `candidate`'s chain to `file`. Strict (the chain check), a missing
/// declaration or module file breaks the chain; lenient (the derivation), it
/// becomes a link the scaffold adds. Any other broken link fails both.
pub(super) fn walk(
    source: FileSource<'_>,
    package: &Package,
    candidate: &Candidate,
    file: &str,
    strict: bool,
) -> Result<Vec<Link>, Failure> {
    let mut links = Vec::new();
    let mut declaring = Declaring::File(candidate.root.path.clone());
    let mut dir = parent(&candidate.root.path).to_string();
    for (index, module) in candidate.modules.iter().enumerate() {
        let fail = |problems: Vec<String>| Failure {
            links: index,
            problems,
        };
        let declared = link_declared(source, &declaring, module, strict).map_err(fail)?;
        let files = [
            join(&dir, &format!("{module}.rs")),
            join(&dir, &format!("{module}/mod.rs")),
        ];
        let next = if index + 1 == candidate.modules.len() {
            only_file(source, &files, file, module).map_err(fail)?;
            None
        } else {
            Some(module_file(source, package, files, module, strict).map_err(fail)?)
        };
        links.push(Link {
            module: module.clone(),
            declaring,
            declared,
        });
        dir = join(&dir, module);
        match next {
            Some(next) => declaring = next,
            None => break,
        }
    }
    Ok(links)
}

/// Whether the link's declaring file declares `module`: a new module root
/// declares nothing yet, and strict, an undeclared module breaks the chain.
fn link_declared(
    source: FileSource<'_>,
    declaring: &Declaring,
    module: &str,
    strict: bool,
) -> Result<bool, Vec<String>> {
    let Declaring::File(path) = declaring else {
        return Ok(false);
    };
    let declared = declares(source, path, module)?;
    if strict && !declared {
        return Err(vec![format!(
            "{path} does not declare {module}: add `mod {module};`"
        )]);
    }
    Ok(declared)
}

/// Whether `path` declares `module` cleanly: `Ok(false)` when it does not
/// declare it at all, `Err` for every rule the file or the declaration
/// breaks.
fn declares(source: FileSource<'_>, path: &str, module: &str) -> Result<bool, Vec<String>> {
    let text = source(path).ok_or_else(|| vec![format!("{path} is missing")])?;
    let parsed = items::parse(&text).map_err(|error| vec![format!("{path}: {error}")])?;
    let mut problems = file_problems(path, &parsed);
    let found: Vec<&ModItem> = parsed
        .mods
        .iter()
        .filter(|item| item.name == module)
        .collect();
    match found.as_slice() {
        [] => {}
        [item] => problems.extend(item_problems(path, item)),
        _ => problems.push(format!(
            "{path} declares {module} {} times: a module on a contract's chain must be declared exactly once",
            found.len()
        )),
    }
    if problems.is_empty() {
        Ok(!found.is_empty())
    } else {
        Err(problems)
    }
}

/// A chain file must parse, and no inner attribute may cfg it out: of the
/// cfg attributes only `#![cfg(test)]` passes, and an inner attribute whose
/// path cannot be read is refused rather than guessed at.
fn file_problems(path: &str, parsed: &ParsedFile) -> Vec<String> {
    let mut problems = Vec::new();
    if parsed.has_error {
        problems.push(format!(
            "{path} does not parse: a file on a contract's chain must be valid Rust"
        ));
    }
    for attribute in &parsed.inner_attributes {
        let rule = match attribute.path.as_deref() {
            None => "loom cannot read its path, and a module on a contract's chain must not be cfg'd out",
            Some(name)
                if name.rsplit("::").next().unwrap_or_default().starts_with("cfg")
                    && attribute.normalised() != "#![cfg(test)]" =>
            {
                "a module on a contract's chain must not be cfg'd out"
            }
            Some(_) => continue,
        };
        problems.push(format!(
            "{path} starts with {}: {rule}",
            quote(&attribute.text)
        ));
    }
    problems
}

/// A link is one out-of-line `mod name;` whose outer attributes are only
/// `#[cfg(test)]`, `#[allow(..)]` and `#[expect(..)]`.
fn item_problems(path: &str, item: &ModItem) -> Vec<String> {
    let name = &item.name;
    let mut problems = Vec::new();
    if item.inline {
        problems.push(format!(
            "{path} declares {name} inline: a module on a contract's chain must be declared out of line, as `mod {name};`"
        ));
    }
    problems.extend(
        item.attributes
            .iter()
            .filter_map(|attribute| attribute_problem(path, name, attribute)),
    );
    problems
}

fn attribute_problem(path: &str, module: &str, attribute: &Attribute) -> Option<String> {
    if attribute.normalised() == "#[cfg(test)]" {
        return None;
    }
    let rule = match attribute.path.as_deref() {
        Some("allow" | "expect") => return None,
        Some("path") => "a contract module must be declared without #[path]",
        Some("cfg_attr") => "a module on a contract's chain must not carry #[cfg_attr(..)]",
        Some("cfg") => "a module on a contract's chain may be cfg'd only as #[cfg(test)]",
        Some(_) => "a module on a contract's chain may carry only #[cfg(test)], #[allow(..)], #[expect(..)] and doc comments",
        None => "loom cannot read its path, and a module on a contract's chain may carry only #[cfg(test)], #[allow(..)], #[expect(..)] and doc comments",
    };
    Some(format!(
        "{path} declares {module} with {}: {rule}",
        quote(&attribute.text)
    ))
}

/// The last link resolves to `file`; the other file of the module must not
/// exist (rustc refuses both, and compiles the other alone if `file` is gone).
fn only_file(
    source: FileSource<'_>,
    files: &[String; 2],
    file: &str,
    module: &str,
) -> Result<(), Vec<String>> {
    match files
        .iter()
        .find(|path| path.as_str() != file && source(path.as_str()).is_some())
    {
        Some(other) => Err(vec![format!(
            "{other} exists: module {module} must be compiled from {file} alone"
        )]),
        None => Ok(()),
    }
}

/// The file of an intermediate module: whichever of `<dir>/m.rs` and
/// `<dir>/m/mod.rs` exists, or, lenient, a new module root.
fn module_file(
    source: FileSource<'_>,
    package: &Package,
    files: [String; 2],
    module: &str,
    strict: bool,
) -> Result<Declaring, Vec<String>> {
    let [flat, nested] = files;
    match (source(&flat).is_some(), source(&nested).is_some()) {
        (true, false) => Ok(Declaring::File(flat)),
        (false, true) => Ok(Declaring::File(nested)),
        (true, true) => Err(vec![format!(
            "both {flat} and {nested} exist: module {module} must have exactly one file"
        )]),
        (false, false) if strict => Err(vec![format!(
            "neither {flat} nor {nested} exists: module {module} has no file"
        )]),
        (false, false) => {
            let mut paths = vec![nested];
            if !is_auto_target(package, &flat) {
                paths.push(flat);
            }
            Ok(Declaring::New(paths))
        }
    }
}

/// A path cargo would take as a target of its own (or as the build script),
/// so a module root is never offered there.
fn is_auto_target(package: &Package, path: &str) -> bool {
    let Some(local) = package.local(path) else {
        return false;
    };
    matches!(local, "src/main.rs" | "src/lib.rs" | "build.rs")
        || ["src/bin", "tests", "examples", "benches"].contains(&parent(local))
}

/// Whether `modules` is a proper prefix of the `::`-separated `test`.
fn prefixes(modules: &[String], test: &str) -> bool {
    let segments: Vec<&str> = test.split("::").collect();
    segments.len() > modules.len()
        && segments.iter().all(|segment| !segment.is_empty())
        && modules
            .iter()
            .zip(&segments)
            .all(|(module, segment)| module == segment)
}

fn shown_path(modules: &[String]) -> String {
    if modules.is_empty() {
        "the crate root".to_string()
    } else {
        modules.join("::")
    }
}

/// A plain ASCII identifier other than `_`.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    name != "_"
        && chars
            .next()
            .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|rest| rest == '_' || rest.is_ascii_alphanumeric())
}

#[cfg(all(test, feature = "source-graph"))]
#[path = "tests_chain.rs"]
mod tests;
