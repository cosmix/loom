//! Host directories a confined command may read: the host caches the stage
//! environment names as read-only seeds, and toolchains installed under HOME.

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
    /// exists and it does not expose a credential path.
    pub(super) fn resolve(host_env: &BTreeMap<String, OsString>) -> Seeds {
        let home = home(host_env);
        let lookup = |name: &str| absolute(host_env, name);
        let under_home = |relative: &str| home.as_ref().map(|home| home.join(relative));
        let credentials = home.as_deref().map(credential_paths).unwrap_or_default();
        let keep = |path: Option<PathBuf>| {
            path.filter(|path| path.is_dir() && !exposes_credentials(path, &credentials))
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
        Seeds {
            go: keep(mod_cache.map(|path| path.join("cache/download"))),
            ccache: keep(ccache),
            maven: keep(under_home(".m2/repository")),
            gradle: keep(gradle),
        }
    }
}

/// The existing seed directories [`super::StageCache::environment`] names,
/// canonical and deduplicated, for a confined command's read-only roots.
pub fn seed_read_roots(host_env: &BTreeMap<String, OsString>) -> Vec<PathBuf> {
    let seeds = Seeds::resolve(host_env);
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
/// HOME, plus `/opt` and `/nix/store`, canonical and deduplicated. HOME
/// itself and every entry that is, lies inside or contains a credential
/// directory are left out.
pub fn toolchain_read_roots(host_env: &BTreeMap<String, OsString>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = home(host_env) {
        let canonical_home = home.canonicalize().unwrap_or_else(|_| home.clone());
        let credentials = credential_paths(&home);
        let path_entries = host_env
            .get("PATH")
            .map(|path| std::env::split_paths(path).collect::<Vec<_>>())
            .unwrap_or_default();
        let candidates = TOOLCHAIN_ROOTS
            .iter()
            .map(|relative| home.join(relative))
            .chain(path_entries.into_iter().filter(|entry| entry.is_absolute()));
        for candidate in candidates {
            let Ok(canonical) = candidate.canonicalize() else {
                continue;
            };
            let below_home = canonical.starts_with(&canonical_home) && canonical != canonical_home;
            if below_home && canonical.is_dir() && !exposes_credentials(&canonical, &credentials) {
                push_unique(&mut roots, canonical);
            }
        }
    }
    for system in SYSTEM_ROOTS.map(Path::new) {
        if system.is_dir() {
            push_canonical(&mut roots, system);
        }
    }
    roots
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
