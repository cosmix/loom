//! The macOS boundary: a Seatbelt profile run through `sandbox-exec`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::NetworkAccess;
use super::{ancestors, git_roots, protected, read_roots, writable_roots, CargoScratch};

/// Seatbelt read, write and deny rules for the tree, scratch, Cargo home, git
/// roots and the command's own read-only roots.
fn seatbelt_path_rules(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    git: Option<&git_roots::GitDirs>,
) -> Result<String> {
    let quote = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();
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
        profile.push_str(&format!("(allow file-read* (subpath {}))", quote(root)));
    }
    let writable: Vec<_> = writable.into_iter().map(quote).collect();
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
            quote(&root.canonicalize()?)
        ));
    }
    let paths = protected::protected(tree);
    profile.push_str(&protected::seatbelt_rules(&paths, quote));
    let hidden = ancestors::hidden_above(tree, &readable);
    profile.push_str(&ancestors::seatbelt_rules(&hidden, quote));
    Ok(profile)
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
