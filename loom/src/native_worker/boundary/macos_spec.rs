//! What one macOS boundary exposes: the Seatbelt spec, the binaries it runs
//! and its environment, derived from the supervisor's `BoundarySpec` and the
//! private invocation root.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::environment::{host_cargo_home, lookup, seatbelt_environment};
use super::mounts::{self, codex_package, SYSTEM_CODEX_CONFIG};
use super::private_root::PrivateRoot;
use super::seatbelt_profile::{HostFiles, SeatbeltBoundarySpec};
use super::BoundarySpec;
use crate::verify::criteria::{git_dirs, read_roots, NetworkAccess};

/// The worktree entries no write may reach: Git metadata, loom state and
/// agent configuration, which later trusted readers follow.
const PROTECTED: [&str; 4] = [".git", ".loom", ".claude", ".codex"];

/// One macOS launch: the profile's spec and the host toolchain files it
/// reaches, the canonical loom and codex executables, and the whole
/// environment.
pub(super) struct MacPlan {
    pub(super) seatbelt: SeatbeltBoundarySpec,
    pub(super) host_files: HostFiles,
    pub(super) loom: PathBuf,
    pub(super) codex: PathBuf,
    pub(super) env: Vec<(OsString, OsString)>,
}

/// The plan for `spec` running in `private`, with the bridge serving
/// `port`. Read roots are the installation and toolchain roots of the
/// boundary's own environment, the canonical loom binary, the codex binary
/// or its validated release directory, and the Git metadata outside the
/// worktree. The real Codex home, the system Codex configuration and the
/// state root (`.loom/work`) are hidden, even below a read root.
pub(super) fn plan(
    spec: &BoundarySpec,
    private: &PrivateRoot,
    port: u16,
    host: &[(OsString, OsString)],
) -> Result<MacPlan> {
    let worktree = spec
        .worktree
        .canonicalize()
        .with_context(|| format!("resolve the worktree {}", spec.worktree.display()))?;
    let loom = spec
        .loom_program
        .canonicalize()
        .with_context(|| format!("resolve the loom binary {}", spec.loom_program.display()))?;
    let package = codex_package(&spec.codex_program)?;
    let home = lookup(host, "HOME").map(PathBuf::from);
    let exposed = [&worktree, &spec.codex_home, &private.root];
    mounts::refuse_wide_package(&package.root, exposed.into_iter().chain(home.as_ref()))?;
    let env = seatbelt_environment(
        host,
        &loom,
        (&private.tmp, &private.codex_home),
        &spec.invocation,
        package.helper_dir.as_deref(),
    );
    let roots = boundary_read_roots(&package.program, &env, &package.root, &worktree);
    let seatbelt = SeatbeltBoundarySpec {
        protected: PROTECTED.iter().map(|name| worktree.join(name)).collect(),
        hidden: hidden(&worktree, &spec.codex_home, home.as_deref()),
        worktree,
        tmp: private.tmp.clone(),
        codex_home: private.codex_home.clone(),
        read_roots: roots,
        proxy_socket: spec.proxy_socket.clone(),
        bridge_port: port,
    };
    Ok(MacPlan {
        seatbelt,
        host_files: HostFiles {
            cargo_home: host_cargo_home(host),
            home,
        },
        loom,
        codex: package.program,
        env,
    })
}

/// The installation and toolchain roots of the boundary environment `env`
/// (probed with `program`), the codex release `package_root` and the Git
/// metadata of `worktree`.
fn boundary_read_roots(
    program: &Path,
    env: &[(OsString, OsString)],
    package_root: &Path,
    worktree: &Path,
) -> Vec<PathBuf> {
    let mut probe = Command::new(program);
    probe.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    let mut roots = read_roots(&probe);
    roots.push(package_root.to_path_buf());
    let git = git_dirs(worktree, NetworkAccess::Denied).map(|dirs| dirs.roots);
    roots.extend(git.unwrap_or_default());
    roots.dedup();
    roots
}

/// The state root of `worktree`, the real Codex home `codex_home` (and
/// `$HOME/.codex` when that differs) and the system Codex configuration.
/// The profile hides each at its own path and its target, then re-allows
/// reads of the map stores below the state root.
fn hidden(worktree: &Path, codex_home: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut hidden = vec![
        worktree.join(".loom/work"),
        codex_home.to_path_buf(),
        PathBuf::from(SYSTEM_CODEX_CONFIG),
    ];
    let default_home = home.map(|home| home.join(".codex"));
    hidden.extend(default_home.filter(|path| path != codex_home));
    hidden
}
