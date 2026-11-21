//! The directories a `PATH` entry's tools read beyond the entry itself: the
//! install root a `bin` directory sits in, and the `bin` directory each
//! executable the entry reaches through a symlink lives in (a version
//! manager's or a tool cache's link into an install).
//!
//! An install root is granted only on positive evidence that it is a
//! toolchain and not a project: a recognised install layout
//! ([`toolchain_layout`]) and none of the [`PROJECT_MARKERS`]. A project's
//! `bin` (a Rails app's `bin/rails`) beside `lib/` grants nothing beyond the
//! entry, so its `.env`, `config/master.key` or `.git` stay unreadable. Every
//! candidate is a path the caller then filters against HOME and the
//! credential directories. An entry or symlink that reaches a writable root
//! ([`Writable`]) is never followed: the command could choose its target.

use super::writable::Writable;
use std::path::{Path, PathBuf};

/// The `<arch>` names `actions/tool-cache` installs under.
const TOOL_CACHE_ARCHES: [&str; 4] = ["x64", "arm64", "x86", "aarch64"];

/// The directory GitHub's hosted runners keep `actions/tool-cache` in.
const HOSTED_TOOL_CACHE: &str = "hostedtoolcache";

/// Files and directories at the root of a project or checkout. A candidate
/// install root holding any of them is refused whatever its layout.
const PROJECT_MARKERS: [&str; 17] = [
    ".git",
    ".hg",
    ".svn",
    ".env",
    "Cargo.toml",
    "package.json",
    "Gemfile",
    "go.mod",
    "pyproject.toml",
    "setup.py",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "settings.gradle",
    "composer.json",
    "Makefile",
    "CMakeLists.txt",
];

/// The candidate roots for the canonical `PATH` entry `entry` below
/// `canonical_home`: its install root, then, for each symlink in it that
/// does not reach a writable root, the target's `bin` directory
/// ([`target_dir_allowed`]) and that directory's install root. `tool_cache`
/// is the canonical `RUNNER_TOOL_CACHE`, when set ([`tool_cache_entry`]).
pub(super) fn install_roots(
    entry: &Path,
    canonical_home: &Path,
    tool_cache: Option<&Path>,
    writable: &Writable,
) -> Vec<PathBuf> {
    let install_root = |bin_dir: &Path| install_root(bin_dir, canonical_home, tool_cache);
    let mut roots: Vec<PathBuf> = install_root(entry).into_iter().collect();
    for directory in symlink_target_dirs(entry, writable) {
        if directory == entry {
            continue;
        }
        roots.extend(install_root(&directory));
        if target_dir_allowed(&directory, canonical_home) {
            roots.push(directory);
        }
    }
    roots
}

/// The parent of `bin_dir` when `bin_dir` is a `bin` or `sbin` directory and
/// the parent is a recognised toolchain install holding no project marker.
/// HOME, anything outside it and a hidden directory directly in HOME
/// (`~/.local`, `~/.cargo`, `~/.bun`, which gather many tools' state beside
/// the `bin` they put on `PATH`) are never one.
fn install_root(
    bin_dir: &Path,
    canonical_home: &Path,
    tool_cache: Option<&Path>,
) -> Option<PathBuf> {
    if !is_bin_dir(bin_dir) {
        return None;
    }
    let parent = bin_dir.parent()?;
    let below_home = parent.starts_with(canonical_home) && parent != canonical_home;
    let hub = parent.parent() == Some(canonical_home) && is_hidden(parent);
    if !below_home || hub || holds_project_marker(parent) {
        return None;
    }
    toolchain_layout(parent, tool_cache).then(|| parent.to_path_buf())
}

/// Whether `dir` has the layout of a toolchain install this module knows:
///
/// - a Go `GOROOT`: `VERSION` and `src/runtime`;
/// - a JDK (on macOS its `Contents/Home`): `release` and `lib/modules`;
/// - a Node install: `bin/node` and `lib/node_modules/npm`;
/// - a Gradle distribution: `bin/gradle` and `lib/plugins` or a
///   `lib/gradle-*.jar`;
/// - an `actions/tool-cache` entry ([`tool_cache_entry`]).
///
/// Version managers keep their installs below the fixed toolchain roots
/// (`roots::TOOLCHAIN_ROOTS`), which need no layout. Revisit the list when a
/// toolchain installed below HOME outside those roots fails to start.
fn toolchain_layout(dir: &Path, tool_cache: Option<&Path>) -> bool {
    let file = |relative: &str| dir.join(relative).is_file();
    let directory = |relative: &str| dir.join(relative).is_dir();
    let go = file("VERSION") && directory("src/runtime");
    let jdk = file("release") && file("lib/modules");
    let node = file("bin/node") && directory("lib/node_modules/npm");
    let gradle = file("bin/gradle") && (directory("lib/plugins") || holds_gradle_jar(dir));
    go || jdk || node || gradle || tool_cache_entry(dir, tool_cache)
}

fn holds_gradle_jar(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir.join("lib")) else {
        return false;
    };
    entries.flatten().any(|item| {
        let name = item.file_name();
        let name = name.to_string_lossy();
        name.starts_with("gradle-") && name.ends_with(".jar")
    })
}

/// Whether `dir` is `<tool>/<version>/<arch>` of an `actions/tool-cache`
/// directory: `<arch>` one of [`TOOL_CACHE_ARCHES`], the finished-install
/// marker `<version>/<arch>.complete` beside it, and the whole below
/// `tool_cache` (the canonical `RUNNER_TOOL_CACHE`) when it is set, else
/// below a [`HOSTED_TOOL_CACHE`] directory. Any directory a command can write
/// could carry the marker, so the location is the evidence.
fn tool_cache_entry(dir: &Path, tool_cache: Option<&Path>) -> bool {
    let (Some(arch), Some(version)) = (dir.file_name(), dir.parent()) else {
        return false;
    };
    let Some(tool) = version.parent() else {
        return false;
    };
    let in_cache = match tool_cache {
        Some(cache) => tool.starts_with(cache) && tool != cache,
        None => tool.ancestors().skip(1).any(|ancestor| {
            ancestor
                .file_name()
                .is_some_and(|name| name == HOSTED_TOOL_CACHE)
        }),
    };
    let mut marker = arch.to_os_string();
    marker.push(".complete");
    in_cache
        && TOOL_CACHE_ARCHES.iter().any(|known| arch == *known)
        && version.join(marker).is_file()
}

fn holds_project_marker(dir: &Path) -> bool {
    PROJECT_MARKERS
        .iter()
        .any(|marker| dir.join(marker).symlink_metadata().is_ok())
}

/// Whether a symlink target's directory may be granted by itself: a `bin` or
/// `sbin` directory strictly below HOME, neither a direct child of HOME
/// (`~/bin/tool -> ~/Downloads/tool` must not grant `~/Downloads`) nor
/// inside a hidden directory directly in HOME (`~/.local/share/...`, beside
/// keyrings). A target in a fixed toolchain root is granted with that root;
/// one in a recognised install, with the install root.
fn target_dir_allowed(dir: &Path, canonical_home: &Path) -> bool {
    let Ok(relative) = dir.strip_prefix(canonical_home) else {
        return false;
    };
    let mut components = relative.components();
    let hidden_hub = components
        .next()
        .is_some_and(|first| first.as_os_str().as_encoded_bytes().starts_with(b"."));
    is_bin_dir(dir) && components.next().is_some() && !hidden_hub
}

fn is_bin_dir(dir: &Path) -> bool {
    dir.file_name()
        .is_some_and(|name| name == "bin" || name == "sbin")
}

fn is_hidden(dir: &Path) -> bool {
    dir.file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."))
}

/// The canonical directory of each executable `entry` reaches through a
/// symlink whose resolution never reaches a writable root, deduplicated; a
/// dangling or unreadable link is skipped.
fn symlink_target_dirs(entry: &Path, writable: &Writable) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(entry) else {
        return Vec::new();
    };
    let mut directories = Vec::new();
    for item in entries.flatten() {
        let link = item.path();
        if !item.file_type().is_ok_and(|kind| kind.is_symlink()) || writable.reached_by(&link) {
            continue;
        }
        let Ok(target) = link.canonicalize() else {
            continue;
        };
        let Some(directory) = target.parent().filter(|_| target.is_file()) else {
            continue;
        };
        if !directories.iter().any(|known| known == directory) {
            directories.push(directory.to_path_buf());
        }
    }
    directories
}
