//! The macOS boundary: a Seatbelt profile run through `sandbox-exec`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::NetworkAccess;
use super::{ancestors, git_roots, protected, read_roots, writable_roots, CargoScratch};

/// `path` as an SBPL string literal. JSON escapes each character below
/// U+0020 (`\u0001`, `\b`), and SBPL's string reader does not document those
/// escapes, so such a path could reach the profile naming another file or fail
/// to parse with an error that names no path. Refusing it fails the command
/// with the path named. Skipping only the ancestor denies was the other
/// option: Cargo would then fail on the visible file as if no rule existed,
/// which hides the cause, and an allow rule cannot be skipped at all.
fn quote(path: &Path) -> Result<String> {
    let text = path.to_string_lossy();
    if text.chars().any(|c| c < ' ') {
        anyhow::bail!(
            "confined verification cannot name {path:?} in a Seatbelt profile: \
             the path holds a control character"
        );
    }
    Ok(serde_json::to_string(&text)?)
}

/// [`quote`] for a rule builder that takes an infallible quoter, once every
/// path it will name has passed [`quote`].
fn quote_checked(paths: &[PathBuf]) -> Result<impl Fn(&Path) -> String> {
    for path in paths {
        quote(path)?;
    }
    Ok(|path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap())
}

/// Seatbelt read, write and deny rules for the tree, scratch, Cargo home, git
/// roots and the command's own read-only roots.
fn seatbelt_path_rules(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    git: Option<&git_roots::GitDirs>,
) -> Result<String> {
    let mut profile = String::new();
    let writable = writable_roots(tree, scratch);
    let readable = writable.iter().map(|path| path.to_path_buf());
    let readable = readable.chain(git.into_iter().flat_map(|git| git.roots.iter().cloned()));
    let readable: Vec<PathBuf> = read_roots(original)
        .into_iter()
        .chain(readable)
        .map(|root| root.canonicalize().unwrap_or(root))
        .collect();
    for root in &readable {
        profile.push_str(&format!("(allow file-read* (subpath {}))", quote(root)?));
    }
    let writable = writable
        .into_iter()
        .map(quote)
        .collect::<Result<Vec<_>>>()?;
    profile.push_str(&format!(
        "(allow file-write* (subpath {}))(allow file-write-data (literal \"/dev/null\"))",
        writable.join(")(subpath ")
    ));
    for root in read_roots(original)
        .into_iter()
        .filter(|p| p != Path::new("/dev"))
    {
        profile.push_str(&format!(
            "(deny file-write* (subpath {}))",
            quote(&root.canonicalize()?)?
        ));
    }
    let paths = protected::protected(tree);
    let named = [&paths.read_only, &paths.hidden_files, &paths.hidden_dirs];
    let named: Vec<PathBuf> = named.into_iter().flatten().cloned().collect();
    profile.push_str(&protected::seatbelt_rules(&paths, quote_checked(&named)?));
    let hidden = ancestors::hidden_above(tree, &readable);
    profile.push_str(&ancestors::seatbelt_rules(&hidden, quote_checked(&hidden)?));
    if let Some(home) = scratch.host_cargo_home() {
        profile.push_str(&host_cargo_home_rules(home)?);
    }
    Ok(profile)
}

/// Files in a Cargo home that never reach a confined command, as Bubblewrap's
/// private home keeps them out: the user's config and registry credentials.
const HIDDEN_IN_CARGO_HOME: [&str; 4] =
    ["config", "config.toml", "credentials", "credentials.toml"];

/// Rules for the host Cargo home read in place. Its lock and last-use files are
/// readable, so Cargo opens them and only warns when it cannot write them;
/// nothing in the home is writable. Its config and credentials files are
/// hidden by name, metadata included: Cargo fails on a config file it can see
/// but not read. Each existing file is hidden at its path and its target.
fn host_cargo_home_rules(home: &Path) -> Result<String> {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let mut rules = String::new();
    for name in super::super::scratch::linked_home::STATE_FILES {
        let state = quote(&home.join(name))?;
        rules.push_str(&format!("(allow file-read* (literal {state}))"));
    }
    let mut hidden = Vec::new();
    for path in HIDDEN_IN_CARGO_HOME.map(|name| home.join(name)) {
        if path.symlink_metadata().is_err() {
            continue;
        }
        match path.canonicalize() {
            Ok(target) if target != path => hidden.push(target),
            _ => {}
        }
        hidden.push(path);
    }
    rules.push_str(&ancestors::seatbelt_rules(&hidden, quote_checked(&hidden)?));
    Ok(rules)
}

pub(super) fn platform_command(
    original: &Command,
    tree: &Path,
    // Seatbelt keeps the process's own working directory, set by `wrap`.
    _cwd: &Path,
    program: &Path,
    scratch: &CargoScratch,
    network: NetworkAccess,
    git: Option<&git_roots::GitDirs>,
) -> Result<Command> {
    let launcher = Path::new("/usr/bin/sandbox-exec");
    if !launcher.is_file() {
        anyhow::bail!("confined verification requires sandbox-exec");
    }
    // dyld opens the root directory while discovering the system Cryptex cache.
    let mut profile = String::from("(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal \"/\"))");
    if network == NetworkAccess::Provision {
        profile.push_str("(allow network-outbound)");
    }
    profile.push_str(&seatbelt_path_rules(original, tree, scratch, git)?);
    let mut command = Command::new(launcher);
    command
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(original.get_args());
    Ok(command)
}

#[cfg(test)]
#[path = "confine_seatbelt_tests.rs"]
mod tests;
