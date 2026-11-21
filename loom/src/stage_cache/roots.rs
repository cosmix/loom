//! Host directories a confined command may read: the host caches the stage
//! environment names as read-only seeds, and toolchains installed under HOME.

use super::install_roots::install_roots;
use super::writable::Writable;
use crate::fs::permissions::state_root::{
    CODEX_AUTH_DENY_READ_PATH, CREDENTIAL_DENY_READ_PATHS, CREDENTIAL_FILE_DENY_READ_PATHS,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Toolchain install roots below HOME that version managers and global
/// package installs write; `PATH` symlinks often point into them.
const TOOLCHAIN_ROOTS: [&str; 14] = [
    ".local/bin",
    ".local/pipx",
    "go/bin",
    "sdk",
    ".bun/install/global",
    ".nvm/versions",
    ".volta",
    ".local/share/fnm",
    ".sdkman/candidates",
    ".asdf/installs",
    ".local/share/mise/installs",
    ".pyenv/versions",
    ".jdks",
    ".deno/bin",
];

/// System-wide install roots, readable whenever they exist.
const SYSTEM_ROOTS: [&str; 2] = ["/opt", "/nix/store"];

/// The existing host caches a stage cache layers over, read-only.
#[derive(Debug, Default)]
pub(super) struct Seeds {
    /// `<GOMODCACHE>/cache/download`, served through a `file://` `GOPROXY`.
    pub(super) go: Option<PathBuf>,
    /// The host ccache directory, a read-only secondary storage.
    pub(super) ccache: Option<PathBuf>,
    /// `~/.m2/repository`, Maven's `maven.repo.local.tail`.
    pub(super) maven: Option<PathBuf>,
    /// `<GRADLE_USER_HOME>/caches` holding `modules-2`, `GRADLE_RO_DEP_CACHE`.
    pub(super) gradle: Option<PathBuf>,
}

impl Seeds {
    /// Resolve the seeds from `host_env`; each is kept only when its directory
    /// exists, it does not expose a credential path and its path as named
    /// reaches none of the `writable` roots ([`Writable::reached_by`]): a stage
    /// session names seeds inside its own cache root, where a confined
    /// command could replace the directory with a symlink to any host
    /// directory. The Go, ccache and Maven seeds go into list syntax
    /// (`GOPROXY`, `CCACHE_REMOTE_STORAGE`, `MAVEN_OPTS`), so one whose path
    /// holds whitespace, `,` or `|` is left out: the tool would read it as
    /// two entries.
    pub(super) fn resolve(host_env: &BTreeMap<String, OsString>, writable: &Writable) -> Seeds {
        let home = home(host_env);
        let lookup = |name: &str| absolute(host_env, name);
        let under_home = |relative: &str| home.as_ref().map(|home| home.join(relative));
        let credentials = home.as_deref().map(credential_paths).unwrap_or_default();
        // A seed is named by its canonical path, the one its read root grants.
        let keep = |path: Option<PathBuf>| {
            path.filter(|path| !writable.reached_by(path))
                .and_then(|path| path.canonicalize().ok())
                .filter(|path| path.is_dir() && !exposes_credentials(path, &credentials))
        };
        let go_path = host_env
            .get("GOPATH")
            .and_then(|value| std::env::split_paths(value).next())
            .filter(|path| path.is_absolute());
        let mod_cache = lookup("GOMODCACHE")
            .or_else(|| go_path.map(|path| path.join("pkg/mod")))
            .or_else(|| under_home("go/pkg/mod"));
        let ccache = lookup("CCACHE_DIR").or_else(|| {
            if cfg!(target_os = "macos") {
                under_home("Library/Caches/ccache")
            } else {
                lookup("XDG_CACHE_HOME")
                    .map(|xdg| xdg.join("ccache"))
                    .or_else(|| under_home(".cache/ccache"))
            }
        });
        let gradle = lookup("GRADLE_USER_HOME")
            .or_else(|| under_home(".gradle"))
            .map(|home| home.join("caches"))
            .filter(|caches| caches.join("modules-2").is_dir());
        let listable = |path: Option<PathBuf>| keep(path).filter(|path| fits_a_list(path));
        Seeds {
            go: listable(mod_cache.map(|path| path.join("cache/download"))),
            ccache: listable(ccache),
            maven: listable(under_home(".m2/repository")),
            gradle: keep(gradle),
        }
    }
}

/// Whether `path` can be an entry of a list separated by whitespace, `,` or `|`.
fn fits_a_list(path: &Path) -> bool {
    !path
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .any(|byte| byte.is_ascii_whitespace() || matches!(byte, b',' | b'|'))
}

/// The existing seed directories [`super::StageCache::environment`] names,
/// canonical and deduplicated, for a confined command's read-only roots. A
/// seed whose path reaches one of the command's `writable` roots, as named or
/// at any symlink hop, is left out ([`Seeds::resolve`]).
pub fn seed_read_roots(host_env: &BTreeMap<String, OsString>, writable: &[&Path]) -> Vec<PathBuf> {
    let seeds = Seeds::resolve(host_env, &Writable::new(writable));
    let mut roots = Vec::new();
    for seed in [seeds.go, seeds.ccache, seeds.maven, seeds.gradle]
        .into_iter()
        .flatten()
    {
        push_canonical(&mut roots, &seed);
    }
    roots
}

/// The existing toolchain install roots and `PATH` entries strictly below
/// HOME, with each entry's own install root and symlink target directories
/// ([`install_roots`]), plus `/opt` and `/nix/store`, canonical, deduplicated
/// and without a root nested in another. HOME itself, every root containing
/// HOME, and every entry that is, lies inside or contains a credential
/// directory are left out. So is every root and `PATH` entry that reaches
/// one of the `writable` roots ([`Writable::reached_by`]): such an entry is
/// not expanded either, since a command writing there chooses its symlinks.
pub fn toolchain_read_roots(
    host_env: &BTreeMap<String, OsString>,
    writable: &[&Path],
) -> Vec<PathBuf> {
    toolchain_read_roots_in(host_env, &SYSTEM_ROOTS.map(Path::new), writable)
}

/// [`toolchain_read_roots`] with `systems` as the system-wide install roots.
pub(super) fn toolchain_read_roots_in(
    host_env: &BTreeMap<String, OsString>,
    systems: &[&Path],
    writable: &[&Path],
) -> Vec<PathBuf> {
    let writable = Writable::new(writable);
    let home = home(host_env);
    let canonical_home = home
        .as_ref()
        .map(|home| home.canonicalize().unwrap_or_else(|_| home.clone()));
    let credentials = home.as_deref().map(credential_paths).unwrap_or_default();
    let tool_cache =
        absolute(host_env, "RUNNER_TOOL_CACHE").map(|cache| cache.canonicalize().unwrap_or(cache));
    let mut roots = Vec::new();
    if let (Some(home), Some(canonical_home)) = (&home, &canonical_home) {
        let bounds = HomeBounds {
            home,
            canonical_home,
            credentials: &credentials,
            tool_cache: tool_cache.as_deref(),
            writable: &writable,
        };
        roots = bounds.roots(host_env);
    }
    let homes: Vec<&PathBuf> = home.iter().chain(&canonical_home).collect();
    for system in systems {
        if writable.reached_by(system) {
            continue;
        }
        let Ok(canonical) = system.canonicalize() else {
            continue;
        };
        let holds_home = homes.iter().any(|home| home.starts_with(&canonical));
        if canonical.is_dir() && !holds_home && !exposes_credentials(&canonical, &credentials) {
            push_unique(&mut roots, canonical);
        }
    }
    outermost(roots)
}

/// What a root below HOME must avoid.
struct HomeBounds<'a> {
    home: &'a Path,
    canonical_home: &'a Path,
    credentials: &'a [PathBuf],
    /// The canonical `RUNNER_TOOL_CACHE`, when set.
    tool_cache: Option<&'a Path>,
    writable: &'a Writable,
}

impl HomeBounds<'_> {
    /// The [`TOOLCHAIN_ROOTS`] and `PATH` entries strictly below HOME, each
    /// `PATH` entry followed by its [`install_roots`], canonical and
    /// deduplicated, every one [`Self::accepts`].
    fn roots(&self, host_env: &BTreeMap<String, OsString>) -> Vec<PathBuf> {
        let path_entries = host_env
            .get("PATH")
            .map(|path| std::env::split_paths(path).collect::<Vec<_>>())
            .unwrap_or_default();
        let fixed = TOOLCHAIN_ROOTS
            .iter()
            .map(|relative| (self.home.join(relative), false));
        let on_path = path_entries
            .into_iter()
            .filter(|entry| entry.is_absolute())
            .map(|entry| (entry, true));
        let mut roots = Vec::new();
        for (candidate, from_path) in fixed.chain(on_path) {
            if self.writable.reached_by(&candidate) {
                continue;
            }
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            if !self.accepts(&canonical) {
                continue;
            }
            let installs = if from_path {
                install_roots(
                    &canonical,
                    self.canonical_home,
                    self.tool_cache,
                    self.writable,
                )
            } else {
                Vec::new()
            };
            push_unique(&mut roots, canonical);
            for install in installs.into_iter().filter(|install| self.accepts(install)) {
                push_unique(&mut roots, install);
            }
        }
        roots
    }

    /// Whether the canonical `path` is a directory strictly below HOME that
    /// exposes no credential path and reaches no writable root.
    fn accepts(&self, path: &Path) -> bool {
        let below_home = path.starts_with(self.canonical_home) && path != self.canonical_home;
        below_home
            && path.is_dir()
            && !exposes_credentials(path, self.credentials)
            && !self.writable.reached_by(path)
    }
}

/// `roots` without any root lying strictly inside another, in order.
fn outermost(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .iter()
        .filter(|root| {
            !roots
                .iter()
                .any(|other| other != *root && root.starts_with(other))
        })
        .cloned()
        .collect()
}

/// An absolute `HOME` from `host_env`.
fn home(host_env: &BTreeMap<String, OsString>) -> Option<PathBuf> {
    absolute(host_env, "HOME")
}

/// The named variable as an absolute path; unset, empty or relative is `None`.
fn absolute(host_env: &BTreeMap<String, OsString>, name: &str) -> Option<PathBuf> {
    host_env
        .get(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Every credential path the default deny-read list names below HOME
/// (`sandbox_config::default_deny_read`), plus the codex credential, each as
/// written and, when it exists, resolved through symlinks.
fn credential_paths(home: &Path) -> Vec<PathBuf> {
    let canonical_home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let names = CREDENTIAL_DENY_READ_PATHS
        .into_iter()
        .chain(CREDENTIAL_FILE_DENY_READ_PATHS)
        .chain([CODEX_AUTH_DENY_READ_PATH])
        .filter_map(|entry| entry.strip_prefix("~/"))
        .map(|entry| entry.strip_suffix("/**").unwrap_or(entry));
    let mut paths = Vec::new();
    for name in names {
        for base in [home, canonical_home.as_path()] {
            let path = base.join(name);
            if let Ok(resolved) = path.canonicalize() {
                push_unique(&mut paths, resolved);
            }
            push_unique(&mut paths, path);
        }
    }
    paths
}

/// Whether `path` is, lies inside or contains one of `credentials`.
fn exposes_credentials(path: &Path, credentials: &[PathBuf]) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    credentials.iter().any(|credential| {
        [path, canonical.as_path()]
            .iter()
            .any(|candidate| candidate.starts_with(credential) || credential.starts_with(candidate))
    })
}

fn push_canonical(roots: &mut Vec<PathBuf>, path: &Path) {
    if let Ok(canonical) = path.canonicalize() {
        push_unique(roots, canonical);
    }
}

fn push_unique(roots: &mut Vec<PathBuf>, path: PathBuf) {
    if !roots.contains(&path) {
        roots.push(path);
    }
}
