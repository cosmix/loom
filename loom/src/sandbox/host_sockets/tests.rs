//! `hidden_paths_in` over injected facts and a fake filesystem, and
//! `scan_sockets` over a temporary directory; nothing here reads the real
//! `/tmp` or `/run`.

use super::*;
use crate::process::sandbox_probe::{skip_unless, unix_socket_bindable};
use std::collections::BTreeSet;
use std::os::unix::net::UnixListener;
use tempfile::TempDir;

/// A host with every optional fact unset, its `/tmp` at `/t`.
fn facts() -> SocketHostFacts {
    SocketHostFacts {
        uid: 1000,
        tmp_root: PathBuf::from("/t"),
        loom_tmux_dir: PathBuf::from("/t/tmux-1000"),
        keep_visible: vec![PathBuf::from("/repo/.worktrees/s1"), PathBuf::from("/repo")],
        ..SocketHostFacts::default()
    }
}

/// A filesystem holding the files `present`, every ancestor of one a
/// directory and each one named `*.sock` a socket, and the symlinks `links`,
/// each `(link, absolute target)`.
#[derive(Default)]
struct FakeHost {
    present: BTreeSet<PathBuf>,
    links: Vec<(PathBuf, PathBuf)>,
}

impl FakeHost {
    fn new(present: &[&str], links: &[(&str, &str)]) -> Self {
        Self {
            present: present.iter().map(PathBuf::from).collect(),
            links: links
                .iter()
                .map(|(link, target)| (PathBuf::from(link), PathBuf::from(target)))
                .collect(),
        }
    }

    /// `path` with every link it passes through replaced by its target.
    fn resolve(&self, path: &Path) -> PathBuf {
        let mut path = path.to_path_buf();
        while let Some((link, target)) = self.links.iter().find(|(link, _)| path.starts_with(link))
        {
            let rest = path.strip_prefix(link).unwrap().to_path_buf();
            path = if rest.as_os_str().is_empty() {
                target.clone()
            } else {
                target.join(rest)
            };
        }
        path
    }
}

impl HostView for FakeHost {
    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        let resolved = self.resolve(path);
        let exists = self.present.iter().any(|file| file.starts_with(&resolved));
        exists.then_some(resolved)
    }

    fn entries(&self, dir: &Path) -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = self
            .present
            .iter()
            .filter_map(|file| file.strip_prefix(dir).ok()?.components().next())
            .map(|child| dir.join(child))
            .collect();
        entries.dedup();
        entries
    }

    fn is_socket(&self, path: &Path) -> bool {
        self.present.contains(path) && path.extension().is_some_and(|ext| ext == "sock")
    }
}

#[test]
fn run_and_var_run_are_hidden_and_other_candidates_only_when_present() {
    let facts = SocketHostFacts {
        xdg_runtime_dir: Some(PathBuf::from("/xdg")),
        ssh_auth_sock: Some(PathBuf::from("/run/user/1000/gcr/ssh")),
        home: Some(PathBuf::from("/h")),
        tmux_tmpdir: Some(PathBuf::from("/tt")),
        daemon_socket: Some(PathBuf::from("/repo/.loom/work/orchestrator.sock")),
        ..facts()
    };
    let host = FakeHost::new(
        &[
            "/run/user/1000/gcr/ssh",
            "/var/run/lock",
            "/xdg/bus",
            "/h/.gnupg/S.gpg-agent",
            "/tt/tmux-1000/default",
            "/t/tmux-1000/default",
            "/repo/.loom/work/orchestrator.sock",
            "/repo/.worktrees/s1/src",
        ],
        &[],
    );

    assert_eq!(
        hidden_paths_in(&facts, &host),
        [
            "/h/.gnupg",
            "/repo/.loom/work/orchestrator.sock",
            "/run",
            "/t/tmux-1000",
            "/tt/tmux-1000",
            "/var/run",
            "/xdg",
        ]
    );
    assert!(hidden_paths_in(&facts, &FakeHost::default()).is_empty());
}

#[test]
fn gnupghome_replaces_the_home_default() {
    let facts = SocketHostFacts {
        gnupghome: Some(PathBuf::from("/keys")),
        home: Some(PathBuf::from("/h")),
        ..facts()
    };
    let host = FakeHost::new(&["/keys/S.gpg-agent", "/h/.gnupg/S.gpg-agent"], &[]);

    let hidden = hidden_paths_in(&facts, &host);

    assert!(hidden.contains(&"/keys".to_string()), "{hidden:?}");
    assert!(!hidden.contains(&"/h/.gnupg".to_string()), "{hidden:?}");
}

#[test]
fn a_tmp_socket_is_hidden_by_its_top_directory_unless_that_is_unsafe() {
    let sockets = [
        "/t/direct.sock",
        "/t/vscode-a/one.sock",
        "/t/vscode-a/deeper/two.sock",
        "/t/claude-1000/bridge/proxy.sock",
        "/t/repo-copy/x.sock",
    ];
    let facts = SocketHostFacts {
        tmp_sockets: sockets.iter().map(PathBuf::from).collect(),
        keep_visible: vec![PathBuf::from("/t/repo-copy/.worktrees/s1")],
        ..facts()
    };
    let mut present = sockets.to_vec();
    present.extend(["/t/.X11-unix/X0", "/t/tmux-1000/default", "/run/lock"]);
    let host = FakeHost::new(&present, &[]);

    assert_eq!(
        hidden_paths_in(&facts, &host),
        [
            "/run",
            "/t/.X11-unix",
            "/t/claude-1000/bridge/proxy.sock",
            "/t/direct.sock",
            "/t/repo-copy/x.sock",
            "/t/tmux-1000",
            "/t/vscode-a",
        ]
    );
}

#[test]
fn a_candidate_holding_tmp_or_the_worktree_or_a_relative_one_is_dropped() {
    let facts = SocketHostFacts {
        xdg_runtime_dir: Some(PathBuf::from("/t")),
        gnupghome: Some(PathBuf::from("/")),
        tmux_tmpdir: Some(PathBuf::from("relative")),
        daemon_socket: Some(PathBuf::from("/repo")),
        ..facts()
    };
    let host = FakeHost::new(
        &["/t/f", "/repo/f", "relative/tmux-1000/default", "/run/lock"],
        &[],
    );

    assert_eq!(hidden_paths_in(&facts, &host), ["/run"]);
}

/// `/var/run` linking to `/run` is listed once, as `/run`; a link at a fixed
/// `/tmp` place pointing at the repository or the worktree's parent hides
/// neither; a link elsewhere is listed by its target, and a dangling one not
/// at all.
#[test]
fn a_symlinked_candidate_is_resolved_before_the_guard() {
    let facts = SocketHostFacts {
        home: Some(PathBuf::from("/h")),
        tmux_tmpdir: Some(PathBuf::from("/tt")),
        ..facts()
    };
    let host = FakeHost::new(
        &[
            "/run/lock",
            "/repo/.worktrees/s1/f",
            "/vault/gnupg/S.gpg-agent",
        ],
        &[
            ("/var/run", "/run"),
            ("/t/tmux-1000", "/repo"),
            ("/t/.X11-unix", "/repo/.worktrees"),
            ("/h/.gnupg", "/vault/gnupg"),
            ("/tt/tmux-1000", "/gone"),
        ],
    );

    assert_eq!(hidden_paths_in(&facts, &host), ["/run", "/vault/gnupg"]);
}

/// A link at a fixed place pointing inside the worktree or the repository
/// hides nothing there; the daemon socket under the state root still is.
#[test]
fn a_candidate_inside_a_kept_path_is_dropped_unless_it_is_a_socket() {
    let socket = "/repo/.loom/work/orchestrator.sock";
    let facts = SocketHostFacts {
        home: Some(PathBuf::from("/h")),
        daemon_socket: Some(PathBuf::from(socket)),
        ..facts()
    };
    let links = [
        ("/t/tmux-1000", "/repo/.worktrees/s1/src"),
        ("/h/.gnupg", "/repo/.loom"),
    ];
    let host = FakeHost::new(&["/repo/.worktrees/s1/src/main.rs", socket], &links);

    assert_eq!(hidden_paths_in(&facts, &host), [socket]);
}

/// The sandbox expands a `denyRead` entry holding `*`, `?`, `[` or `]`; `{`
/// it takes as written.
#[test]
fn a_path_the_sandbox_reads_as_a_glob_is_left_out() {
    let sockets = [
        "/t/a[1]/s.sock",
        "/t/x*y.sock",
        "/t/q?/s.sock",
        "/t/b{1}/s.sock",
    ];
    let facts = SocketHostFacts {
        tmp_sockets: sockets.iter().map(PathBuf::from).collect(),
        ..facts()
    };

    let hidden = hidden_paths_in(&facts, &FakeHost::new(&sockets, &[]));

    assert_eq!(hidden, ["/t/b{1}"]);
}

#[test]
fn a_kept_path_is_guarded_in_its_canonical_spelling() {
    let facts = SocketHostFacts {
        xdg_runtime_dir: Some(PathBuf::from("/srv")),
        keep_visible: vec![PathBuf::from("/w/repo")],
        ..facts()
    };
    let host = FakeHost::new(
        &["/srv/work/repo/f", "/srv/other/bus"],
        &[("/w", "/srv/work")],
    );

    assert!(hidden_paths_in(&facts, &host).is_empty());
}

/// A repository on a removable disk under `/run/media`: every entry on the
/// way down to it that is not on that way is hidden, the repository and a
/// symlink entry are not, and `$XDG_RUNTIME_DIR` folds into `/run/user`.
#[test]
fn run_holding_a_kept_path_is_hidden_around_it() {
    let facts = SocketHostFacts {
        xdg_runtime_dir: Some(PathBuf::from("/run/user/1000")),
        keep_visible: vec![
            PathBuf::from("/run/media/u/disk/repo/.worktrees/s1"),
            PathBuf::from("/run/media/u/disk/repo"),
        ],
        ..facts()
    };
    let host = FakeHost::new(
        &[
            "/run/docker.sock",
            "/run/user/1000/bus",
            "/run/media/u/disk/repo/.worktrees/s1/f",
            "/run/media/u/disk/repo/src/main.rs",
            "/run/media/u/disk/other/f",
            "/run/media/u/stick/f",
            "/run/media/v/f",
            "/etc/passwd",
        ],
        &[("/var/run", "/run"), ("/run/media/u/link", "/etc")],
    );

    assert_eq!(
        hidden_paths_in(&facts, &host),
        [
            "/run/docker.sock",
            "/run/media/u/disk/other",
            "/run/media/u/stick",
            "/run/media/v",
            "/run/user",
        ]
    );
}

#[test]
fn the_walk_around_a_kept_path_stops_at_its_entry_cap() {
    let facts = SocketHostFacts {
        keep_visible: vec![PathBuf::from("/run/k")],
        ..facts()
    };
    let mut present: Vec<String> = (0..=RUN_WALK_MAX_ENTRIES)
        .map(|n| format!("/run/e{n:05}"))
        .collect();
    present.push("/run/k/f".to_string());
    let present: Vec<&str> = present.iter().map(String::as_str).collect();

    let hidden = hidden_paths_in(&facts, &FakeHost::new(&present, &[]));

    assert_eq!(hidden.len(), RUN_WALK_MAX_ENTRIES);
    assert!(!hidden.iter().any(|path| path.starts_with("/run/k")));
}

fn bind(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    drop(UnixListener::bind(path).unwrap());
}

#[test]
fn the_scan_finds_sockets_three_levels_down_and_never_follows_a_symlink() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    if skip_unless(
        unix_socket_bindable(root),
        "sandbox::host_sockets::tests::the_scan_finds_sockets_three_levels_down",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    for socket in [
        "a.sock",
        "d1/b.sock",
        "d1/d2/c.sock",
        "d1/d2/d3/too-deep.sock",
    ] {
        bind(&root.join(socket));
    }
    std::fs::write(root.join("d1/plain"), "").unwrap();
    std::os::unix::fs::symlink(root.join("d1"), root.join("link")).unwrap();

    let mut found = scan_sockets(root, TMP_SCAN_DEPTH, TMP_SCAN_MAX_ENTRIES);
    found.sort();

    let expected: Vec<PathBuf> = ["a.sock", "d1/b.sock", "d1/d2/c.sock"]
        .iter()
        .map(|socket| root.join(socket))
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn the_scan_stops_at_its_entry_cap() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    if skip_unless(
        unix_socket_bindable(root),
        "sandbox::host_sockets::tests::the_scan_stops_at_its_entry_cap",
        "this sandbox denies binding an AF_UNIX listener",
    ) {
        return;
    }
    for socket in ["a.sock", "b.sock", "c.sock", "d.sock"] {
        bind(&root.join(socket));
    }

    assert_eq!(scan_sockets(root, TMP_SCAN_DEPTH, 2).len(), 2);
    assert!(scan_sockets(&root.join("missing"), TMP_SCAN_DEPTH, 2).is_empty());
}

/// The live filesystem leaves a symlink out of a directory's entries and
/// resolves one to its target.
#[test]
fn the_live_host_skips_symlink_entries_and_resolves_links() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("dir")).unwrap();
    std::fs::write(root.join("file"), "").unwrap();
    std::os::unix::fs::symlink(root.join("dir"), root.join("link")).unwrap();

    let mut entries = LiveHost.entries(&root);
    entries.sort();

    assert_eq!(entries, [root.join("dir"), root.join("file")]);
    assert_eq!(
        LiveHost.canonical(&root.join("link")),
        Some(root.join("dir"))
    );
    assert_eq!(LiveHost.canonical(&root.join("missing")), None);
    assert!(!LiveHost.is_socket(&root.join("file")));
}
