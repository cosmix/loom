//! The host's Unix sockets, hidden from a session whose stage is granted every
//! Unix socket.
//!
//! On Linux `sandbox.network.allow_all_unix_sockets` lifts the seccomp filter
//! that refuses `socket(AF_UNIX)`, and with it the only barrier between a
//! sandboxed command and the sockets the sandbox's read-only view of the host
//! root exposes: `connect` needs no write access. The Docker socket (root
//! through the docker group), the session D-Bus (`systemd-run --user` runs
//! outside the sandbox), the ssh and gpg agents, tmux servers and the loom
//! daemon would all answer. Each path [`hidden_socket_paths`] lists goes into
//! the capsule's `sandbox.filesystem.denyRead`, which hides it: a directory
//! behind a tmpfs (the session's own `allowWrite` paths inside it are mounted
//! again), a file behind `/dev/null`.
//!
//! Every listed path is canonical. A symlink is resolved before the guard
//! that keeps the session's working directory and repository visible, so a
//! link planted at a fixed place (`/tmp/tmux-<uid>`) cannot get either one,
//! or anything inside them but a socket, hidden, and `/var/run` collapses
//! into `/run` where it links there. `/run` and `/var/run` are hidden whole
//! unless one holds the working directory or the repository (a removable disk
//! under `/run/media`): then every entry on the way down to it that is not on
//! that way is hidden instead. A path holding a character the sandbox reads
//! as a glob is left out with a warning: the sandbox would expand it.
//!
//! `/tmp` cannot be hidden whole: the sandbox's network proxy keeps its bridge
//! sockets there. A socket found there when the capsule is written is hidden
//! by its top-level directory, or alone when it sits directly in `/tmp` or in
//! a `claude-` directory (the sandbox's own `TMPDIR` area). A socket created
//! under `/tmp` after the session starts, or more than [`TMP_SCAN_DEPTH`]
//! levels down, stays reachable, as does every socket outside `/run`,
//! `/var/run` and `/tmp` this list does not name.

use std::collections::VecDeque;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use super::MergedSandboxConfig;

/// Where host sockets are looked for on a real host.
pub(crate) const HOST_TMP_ROOT: &str = "/tmp";
/// How many levels below the scan root a socket is looked for.
pub(crate) const TMP_SCAN_DEPTH: usize = 3;
/// The most directory entries one scan visits, so a huge `/tmp` cannot stall
/// a spawn.
pub(crate) const TMP_SCAN_MAX_ENTRIES: usize = 10_000;
/// The most entries the walk down to a kept path under `/run` visits.
const RUN_WALK_MAX_ENTRIES: usize = 1_000;

/// Hidden whole, or around the kept paths they hold.
const ALWAYS_HIDDEN: [&str; 2] = ["/run", "/var/run"];
/// A top-level `/tmp` directory with this prefix holds the sandbox's own
/// `TMPDIR` and other sessions' bridge sockets: only the sockets found in it
/// are hidden, never the directory.
const SANDBOX_TMP_PREFIX: &str = "claude-";
/// The characters that make the sandbox runtime expand a Linux `denyRead`
/// entry as a glob (`containsGlobChars`) instead of mounting over it as
/// written.
const SANDBOX_GLOB_CHARS: [char; 4] = ['*', '?', '[', ']'];

/// Whether `config` lets a session create and connect to every Unix socket.
pub(crate) fn grants_all_unix_sockets(config: &MergedSandboxConfig) -> bool {
    config.enabled && config.network.allow_all_unix_sockets
}

/// What `hidden_socket_paths` reads, resolved by the caller.
#[derive(Debug, Clone, Default)]
pub(crate) struct SocketHostFacts {
    pub uid: u32,
    /// `/tmp` on a real host.
    pub tmp_root: PathBuf,
    pub xdg_runtime_dir: Option<PathBuf>,
    /// The daemon's captured `SSH_AUTH_SOCK` (`git::signing::SigningEnv`).
    pub ssh_auth_sock: Option<PathBuf>,
    /// The daemon's captured `GNUPGHOME`; `~/.gnupg` when unset.
    pub gnupghome: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub tmux_tmpdir: Option<PathBuf>,
    /// The directory loom's own tmux servers bind in.
    pub loom_tmux_dir: PathBuf,
    /// The project's daemon socket, in its resolved spelling.
    pub daemon_socket: Option<PathBuf>,
    /// Every socket `scan_sockets` found under `tmp_root`.
    pub tmp_sockets: Vec<PathBuf>,
    /// Paths that must stay visible to the session: its working directory
    /// and the repository. A path that is one of them, holds one, or lies
    /// inside one without being a socket, in its own spelling or its
    /// canonical one, is never hidden; neither is `tmp_root` or a path
    /// holding it.
    pub keep_visible: Vec<PathBuf>,
}

/// What `hidden_socket_paths` reads from the host's filesystem.
trait HostView {
    /// `path` with every symlink resolved; `None` when it does not resolve.
    fn canonical(&self, path: &Path) -> Option<PathBuf>;
    /// The entries of the directory `dir` that are not symlinks; none when
    /// it cannot be read.
    fn entries(&self, dir: &Path) -> Vec<PathBuf>;
    /// Whether `path` is a socket file.
    fn is_socket(&self, path: &Path) -> bool;
}

/// The host's own filesystem.
struct LiveHost;

impl HostView for LiveHost {
    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }

    fn entries(&self, dir: &Path) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| !kind.is_symlink()))
            .map(|entry| entry.path())
            .collect()
    }

    fn is_socket(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
    }
}

/// The `denyRead` entries that hide the host's sockets described by `facts`:
/// canonical, sorted, each once and none inside another. `/run` and
/// `/var/run` are hidden around the kept paths they hold; every other
/// candidate is listed only when it is absolute, resolves, and its canonical
/// path neither holds a kept path nor lies inside one `keep_visible` names
/// without being a socket. A path that is not valid UTF-8 cannot be written
/// into the settings and is left out, as is one the sandbox would read as a
/// glob.
pub(crate) fn hidden_socket_paths(facts: &SocketHostFacts) -> Vec<String> {
    hidden_paths_in(facts, &LiveHost)
}

fn hidden_paths_in(facts: &SocketHostFacts, host: &impl HostView) -> Vec<String> {
    let visible = spellings(&facts.keep_visible, host);
    let mut kept = visible.clone();
    kept.extend(spellings(std::slice::from_ref(&facts.tmp_root), host));
    let mut roots: Vec<PathBuf> = ALWAYS_HIDDEN
        .iter()
        .filter_map(|root| host.canonical(Path::new(root)))
        .collect();
    roots.sort();
    roots.dedup();
    let mut paths = Vec::new();
    for root in &roots {
        hide_around_kept(root, &kept, host, &mut paths);
    }
    paths.extend(
        host_candidates(facts)
            .into_iter()
            .chain(
                facts
                    .tmp_sockets
                    .iter()
                    .filter_map(|socket| tmp_cover(facts, &kept, host, socket)),
            )
            .filter(|path| path.is_absolute())
            .filter_map(|path| host.canonical(&path))
            .filter(|path| !holds_kept(&kept, path) && !inside_visible(&visible, host, path)),
    );
    outermost(paths)
        .into_iter()
        .filter_map(|path| path.to_str().map(str::to_owned))
        .filter(|path| sandbox_reads_literally(path))
        .collect()
}

/// Each of `paths` in its own spelling and, where that differs, its
/// canonical one.
fn spellings(paths: &[PathBuf], host: &impl HostView) -> Vec<PathBuf> {
    let mut spelled = Vec::new();
    for path in paths {
        spelled.push(path.clone());
        spelled.extend(host.canonical(path).filter(|canonical| canonical != path));
    }
    spelled
}

/// Whether `path` lies inside one of the `visible` paths and is no socket:
/// hiding it would hide part of the session's own tree, as a link planted at
/// a fixed place (`/tmp/tmux-<uid>` -> `<repo>/src`) would arrange. A socket
/// there, the daemon's under the repository's state root, is still hidden.
fn inside_visible(visible: &[PathBuf], host: &impl HostView, path: &Path) -> bool {
    visible.iter().any(|kept| path.starts_with(kept)) && !host.is_socket(path)
}

/// Whether the sandbox mounts over `path` as written. It expands an entry
/// holding one of [`SANDBOX_GLOB_CHARS`] against the filesystem, which could
/// hide nothing or more than `path`; such a path is left out, with a warning.
fn sandbox_reads_literally(path: &str) -> bool {
    if !path.contains(SANDBOX_GLOB_CHARS) {
        return true;
    }
    tracing::warn!(
        path,
        "a host socket path holds a character the sandbox reads as a glob, so it is not \
         hidden from a session granted every Unix socket"
    );
    false
}

/// Push `root` when it holds no kept path. Otherwise push, one level at a
/// time on the way down to each kept path it holds, every entry that is not
/// on that way, and leave each kept path whole. A symlink entry stays: it
/// exposes only its target, which is hidden or not on its own terms. The
/// walk stops after `RUN_WALK_MAX_ENTRIES` entries, warning that the rest
/// stay visible.
fn hide_around_kept(root: &Path, kept: &[PathBuf], host: &impl HostView, out: &mut Vec<PathBuf>) {
    if !holds_kept(kept, root) {
        out.push(root.to_path_buf());
        return;
    }
    let mut pending = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(dir) = pending.pop() {
        if kept.contains(&dir) {
            continue;
        }
        for entry in host.entries(&dir) {
            visited += 1;
            if visited > RUN_WALK_MAX_ENTRIES {
                tracing::warn!(
                    root = %root.display(),
                    cap = RUN_WALK_MAX_ENTRIES,
                    "the walk hiding host sockets around a kept path stopped at its entry \
                     cap; the entries past it stay visible to the session"
                );
                return;
            }
            if holds_kept(kept, &entry) {
                pending.push(entry);
            } else {
                out.push(entry);
            }
        }
    }
}

/// The fixed places host sockets live, before any existence check.
fn host_candidates(facts: &SocketHostFacts) -> Vec<PathBuf> {
    let tmux_dir = format!("tmux-{}", facts.uid);
    let gnupg = facts
        .gnupghome
        .clone()
        .or_else(|| facts.home.as_ref().map(|home| home.join(".gnupg")));
    let mut candidates: Vec<PathBuf> = [
        facts.xdg_runtime_dir.clone(),
        facts.ssh_auth_sock.clone(),
        gnupg,
        facts.tmux_tmpdir.as_ref().map(|dir| dir.join(&tmux_dir)),
        facts.daemon_socket.clone(),
    ]
    .into_iter()
    .flatten()
    .collect();
    candidates.push(facts.tmp_root.join(&tmux_dir));
    candidates.push(facts.loom_tmux_dir.clone());
    candidates.push(facts.tmp_root.join(".X11-unix"));
    candidates
}

/// What hides `socket`, found under `facts.tmp_root`: the socket itself when
/// it sits directly there, in a `claude-` directory, or in a directory whose
/// canonical path holds a kept path; its top-level directory otherwise.
fn tmp_cover(
    facts: &SocketHostFacts,
    kept: &[PathBuf],
    host: &impl HostView,
    socket: &Path,
) -> Option<PathBuf> {
    let relative = socket.strip_prefix(&facts.tmp_root).ok()?;
    let mut parts = relative.components();
    let top = parts.next()?;
    if parts.next().is_none() {
        return Some(socket.to_path_buf());
    }
    let top_dir = facts.tmp_root.join(top);
    let sandbox_owned = top
        .as_os_str()
        .to_str()
        .is_some_and(|name| name.starts_with(SANDBOX_TMP_PREFIX));
    let holds = host
        .canonical(&top_dir)
        .is_none_or(|dir| holds_kept(kept, &dir));
    if sandbox_owned || holds {
        Some(socket.to_path_buf())
    } else {
        Some(top_dir)
    }
}

/// Whether hiding `path` would hide one of the `kept` paths.
fn holds_kept(kept: &[PathBuf], path: &Path) -> bool {
    kept.iter().any(|kept| kept.starts_with(path))
}

/// `paths` sorted, without duplicates and without any path inside another.
fn outermost(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    let mut kept: Vec<PathBuf> = Vec::new();
    for path in paths {
        if kept.last().is_none_or(|last| !path.starts_with(last)) {
            kept.push(path);
        }
    }
    kept
}

/// Every socket at most `max_depth` levels below `root`, breadth first,
/// visiting at most `max_entries` directory entries and warning when that cap
/// stops it. Symlinks are never followed and an unreadable directory is
/// skipped.
pub(crate) fn scan_sockets(root: &Path, max_depth: usize, max_entries: usize) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut visited = 0usize;
    while let Some((dir, depth)) = pending.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > max_entries {
                tracing::warn!(
                    root = %root.display(),
                    cap = max_entries,
                    "the host socket scan stopped at its entry cap; a socket it did not \
                     reach stays reachable from a session granted every Unix socket"
                );
                return found;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_socket() {
                found.push(entry.path());
            } else if kind.is_dir() && depth + 1 < max_depth {
                pending.push_back((entry.path(), depth + 1));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests;
