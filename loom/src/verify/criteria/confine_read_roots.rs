//! The read-only roots of a confined command, from its own environment:
//! system and runtime installation directories, Cargo and rustup state, and
//! the host build-cache seeds and home-installed toolchains a stage cache
//! layers over (`crate::stage_cache`).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::stage_cache::{seed_read_roots, toolchain_read_roots};

pub(crate) fn environment_path(command: &Command, key: &str) -> Option<PathBuf> {
    command
        .get_envs()
        .find_map(|(name, value)| (name == key).then_some(value).flatten().map(PathBuf::from))
}

pub(crate) fn cargo_home(command: &Command) -> Option<PathBuf> {
    environment_path(command, "CARGO_HOME")
        .or_else(|| environment_path(command, "HOME").map(|home| home.join(".cargo")))
}

/// The variables `command` sets, by name; removed and non-UTF-8 names are skipped.
pub(crate) fn command_environment(command: &Command) -> BTreeMap<String, OsString> {
    command
        .get_envs()
        .filter_map(|(name, value)| Some((name.to_str()?.to_string(), value?.to_os_string())))
        .collect()
}

/// The existing installation roots of `command`, without the host seeds and
/// toolchain roots [`read_roots_beside`] adds: a native worker's boundary binds
/// these beside writable roots it chooses itself (Linux binds them read-only,
/// the macOS Seatbelt profile allows reading below them).
#[cfg(any(target_os = "linux", target_os = "macos", test))]
pub(crate) fn read_roots(command: &Command) -> Vec<PathBuf> {
    existing(installation_roots(command))
}

/// The existing read-only roots of a confined command: its installation roots,
/// then the host seeds and toolchain roots its environment names, less any
/// that is, contains or lies inside one of the canonical `writable` roots. A
/// launcher applies read-only roots after the writable ones, so such a root
/// would make part of the tree, or all of it, read-only. Seeds resolve from
/// `seed_env`, loom's own host environment, when a stage cache names them
/// from it, else from the command's environment; toolchain roots always come
/// from the command's own `PATH`. Neither keeps a seed path, `PATH` entry or
/// symlink that reaches a writable root, as named or at any hop: the command
/// could plant a link there (`stage_cache::Writable`).
pub(crate) fn read_roots_beside(
    command: &Command,
    writable: &[&Path],
    seed_env: Option<&BTreeMap<String, OsString>>,
) -> Vec<PathBuf> {
    let mut roots = installation_roots(command);
    let mut environment = command_environment(command);
    drop_loom_directory(&mut environment);
    let host_roots = seed_read_roots(seed_env.unwrap_or(&environment), writable)
        .into_iter()
        .chain(toolchain_read_roots(&environment, writable));
    for root in host_roots {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
        let overlaps = writable
            .iter()
            .any(|writable| canonical.starts_with(writable) || writable.starts_with(&canonical));
        if !overlaps && !roots.contains(&root) {
            roots.push(root);
        }
    }
    existing(roots)
}

/// Removes the running loom's directory from `PATH`. The preparer puts it
/// first so `loom ...` criteria resolve, but only the binary itself
/// (`LOOM_BIN`) is a read root: the directory it was built or installed in can
/// hold unrelated user files.
fn drop_loom_directory(environment: &mut BTreeMap<String, OsString>) {
    let directory = environment
        .get("LOOM_BIN")
        .and_then(|bin| Path::new(bin).parent().map(Path::to_path_buf));
    let (Some(directory), Some(path)) = (directory, environment.get("PATH")) else {
        return;
    };
    let kept = std::env::split_paths(path).filter(|entry| *entry != directory);
    // The entries came from one PATH, so they contain no separator.
    let path = std::env::join_paths(kept).unwrap_or_default();
    environment.insert("PATH".to_string(), path);
}

fn existing(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.into_iter().filter(|p| p.exists()).collect()
}

fn installation_roots(command: &Command) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc",
        "/System",
        "/Library",
        "/dev",
        "/opt/homebrew",
        "/private/var/db/timezone",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = environment_path(command, "HOME") {
        roots.extend([
            home.join(".rustup"),
            home.join(".bun/bin"),
            home.join(".bun/install/cache"),
            home.join(".cache/uv"),
            home.join("Library/Caches/uv"),
            home.join(".local/share/uv/python"),
        ]);
    }
    if let Some(rustup) = environment_path(command, "RUSTUP_HOME") {
        roots.push(rustup);
    }
    if let Some(cargo) = cargo_home(command) {
        roots.extend([cargo.join("bin"), cargo.join("registry"), cargo.join("git")]);
    }
    // Expose only the running binary, not its installation directory (which
    // can contain unrelated user files). The preparer owns this environment entry.
    if let Some(executable) = environment_path(command, "LOOM_BIN") {
        roots.push(executable);
    }
    #[cfg(target_os = "macos")]
    roots.extend(super::developer_tools::developer_tools());
    roots
}
