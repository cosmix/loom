//! The Seatbelt profile of the macOS native boundary. It wraps the trusted
//! bridge and everything Codex starts: one profile is the only boundary, as
//! macOS refuses a sandbox started inside another one.
//!
//! Rule order is the policy: Seatbelt applies the last matching rule. The
//! profile denies by default, then allows runtime access, the two network
//! endpoints, reads below the read roots and writes below the writable roots;
//! after those come the protected-path write denies, the hidden-path denies
//! and, last, the read-only re-allows of the map stores below a hidden path.
//! Every path rule names each spelling a process may use (`spellings`).
//! The profile renders on every host, so its contracts run on Linux too.
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// What one macOS boundary may reach. Every root is an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatbeltBoundarySpec {
    /// The canonical worktree: readable and writable, less `protected`.
    pub worktree: PathBuf,
    /// The canonical private temp directory (`TMPDIR`): readable, writable.
    pub tmp: PathBuf,
    /// The canonical private Codex home: readable; only `sessions/` below it
    /// is writable.
    pub codex_home: PathBuf,
    /// Read-only roots: installation and toolchain directories, the loom and
    /// codex binaries or the codex release, and Git metadata.
    pub read_roots: Vec<PathBuf>,
    /// Paths no write may reach, applied after the writable roots; a symlink
    /// is protected at its own path and at its target.
    pub protected: Vec<PathBuf>,
    /// Paths neither read nor written, applied after every allow; a symlink
    /// is hidden at its own path and at its target.
    pub hidden: Vec<PathBuf>,
    /// The proxy's Unix socket: the only socket the boundary may connect to.
    pub proxy_socket: PathBuf,
    /// The bridge's prebound loopback port: the only IP endpoint reachable.
    pub bridge_port: u16,
}

/// dyld opens the root directory while discovering the system Cryptex cache,
/// hence the literal `/`. Metadata stays readable everywhere: tools stat the
/// directories above their working directory; contents stay denied.
const BASE_RULES: &str = "(version 1)(deny default)\
(allow file-read-metadata)(allow file-read* (literal \"/\"))";

/// Processes start, fork and inspect only their own sandbox; `same-sandbox`
/// signals let Codex stop the commands it starts, never the supervisor, the
/// daemon or another user process. `sysctl-read` stays unfiltered: runtimes
/// read CPU, memory and OS-version names that differ per release, and no
/// sysctl is writable.
const PROCESS_RULES: &str = "(allow process-exec process-fork)\
(allow process-info* (target same-sandbox))\
(allow signal (target same-sandbox))(allow sysctl-read)";

/// The Mach services the app-server and ordinary build tools need: user and
/// group lookups, the TLS trust evaluation Codex runs for its tunnelled
/// requests, and notifications (time zone changes, libinfo cache flushes).
/// No DNS: every outbound request goes through the proxy, which resolves.
/// Revisit when the installed-Codex lifecycle test reports a denied lookup.
const MACH_SERVICES: [&str; 8] = [
    "com.apple.system.opendirectoryd.libinfo",
    "com.apple.system.opendirectoryd.membership",
    "com.apple.system.notification_center",
    "com.apple.trustd",
    "com.apple.trustd.agent",
    "com.apple.SecurityServer",
    "com.apple.ocspd",
    "com.apple.cfprefsd.daemon",
];

/// The writable Codex home part: rollouts and session state.
const SESSIONS: &str = "sessions";

/// The profile confining one macOS native boundary to `spec`. Refuses a
/// non-canonical or symlinked writable root, overlapping writable roots, a
/// protected or hidden path that contains a writable root, a proxy socket
/// below a writable root and any path holding a control character.
pub fn profile(spec: &SeatbeltBoundarySpec) -> Result<String> {
    let sessions = validate(spec)?;
    let mut rules = String::from(BASE_RULES);
    rules.push_str(PROCESS_RULES);
    let services: Vec<String> = MACH_SERVICES
        .iter()
        .map(|name| format!("(global-name \"{name}\")"))
        .collect();
    rules.push_str(&format!("(allow mach-lookup {})", services.concat()));
    rules.push_str(&network_rules(spec)?);
    let readable = [&spec.worktree, &spec.tmp, &spec.codex_home]
        .into_iter()
        .chain(&spec.read_roots);
    for root in readable {
        rules.push_str(&rule(
            "allow file-read*",
            "subpath",
            std::slice::from_ref(root),
        )?);
    }
    let writable = [spec.worktree.clone(), spec.tmp.clone(), sessions];
    rules.push_str(&rule("allow file-write*", "subpath", &writable)?);
    rules.push_str("(allow file-write-data (literal \"/dev/null\"))");
    rules.push_str(&rule(
        "deny file-write*",
        "subpath",
        &with_targets(&spec.protected),
    )?);
    let hidden = with_targets(&spec.hidden);
    rules.push_str(&rule("deny file-read* file-write*", "subpath", &hidden)?);
    let maps = super::mounts::map_store_roots(&spec.worktree);
    rules.push_str(&rule("allow file-read*", "subpath", &maps)?);
    Ok(rules)
}

/// Outbound only to the bridge's exact loopback port and the proxy socket in
/// every spelling; inbound only to that port, where the bridge accepts on the
/// listener it inherited already bound, so no bind is granted at all. The
/// loopback name covers `127.0.0.1` and `::1`, never another port.
fn network_rules(spec: &SeatbeltBoundarySpec) -> Result<String> {
    let port = spec.bridge_port;
    anyhow::ensure!(port != 0, "the bridge port must be a bound port, not 0");
    let mut rules = format!(
        "(allow network-outbound (remote ip \"localhost:{port}\"))\
         (allow network-inbound (local ip \"localhost:{port}\"))"
    );
    let mut sockets = vec![spec.proxy_socket.clone()];
    let parent = spec.proxy_socket.parent();
    let name = spec.proxy_socket.file_name();
    if let (Some(parent), Some(name)) = (parent, name) {
        if let Ok(canonical) = parent.canonicalize() {
            sockets.push(canonical.join(name));
        }
    }
    let mut spelled: Vec<PathBuf> = Vec::new();
    for socket in sockets.iter().flat_map(|socket| spellings(socket)) {
        if spelled.contains(&socket) {
            continue;
        }
        spelled.push(socket.clone());
        rules.push_str(&format!(
            "(allow network-outbound (remote unix-socket (path-literal {})))",
            quote(&socket)?
        ));
    }
    Ok(rules)
}

/// One rule `head` with a `filter` per spelling of each path; empty when
/// `paths` is.
fn rule(head: &str, filter: &str, paths: &[PathBuf]) -> Result<String> {
    let mut filters = Vec::new();
    for path in paths {
        for spelling in spellings(path) {
            let filter = format!("({filter} {})", quote(&spelling)?);
            if !filters.contains(&filter) {
                filters.push(filter);
            }
        }
    }
    if filters.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("({head} {})", filters.concat()))
}

/// `paths` and the canonical target of each one that resolves elsewhere.
fn with_targets(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut all = Vec::new();
    for path in paths {
        all.push(path.clone());
        if let Ok(target) = path.canonicalize() {
            if target != *path {
                all.push(target);
            }
        }
    }
    all
}

/// Every spelling a process may use for `path`: the `/private` and public
/// forms of `/var`, `/tmp` and `/etc`, and the Data-volume firmlink of each.
/// `verify::criteria::confine_ancestors::seatbelt_spellings` maps only from
/// `/private`; a spec may name either form, so both directions are needed.
pub(crate) fn spellings(path: &Path) -> Vec<PathBuf> {
    let mut forms = vec![path.to_path_buf()];
    for (private, public) in [
        ("/private/var", "/var"),
        ("/private/tmp", "/tmp"),
        ("/private/etc", "/etc"),
    ] {
        let swapped = match (path.strip_prefix(private), path.strip_prefix(public)) {
            (Ok(rest), _) => Some(join_rest(public, rest)),
            (_, Ok(rest)) => Some(join_rest(private, rest)),
            _ => None,
        };
        forms.extend(swapped);
    }
    let firmlinked: Vec<PathBuf> = forms
        .iter()
        .filter(|form| !form.starts_with("/System/Volumes"))
        .filter_map(|form| form.strip_prefix("/").ok())
        .map(|rest| Path::new("/System/Volumes/Data").join(rest))
        .collect();
    forms.extend(firmlinked);
    forms
}

fn join_rest(base: &str, rest: &Path) -> PathBuf {
    if rest.as_os_str().is_empty() {
        PathBuf::from(base)
    } else {
        Path::new(base).join(rest)
    }
}

/// `path` as an SBPL string literal; the one quoter of every Seatbelt
/// profile loom writes (also `verify::criteria::confine_seatbelt`). JSON
/// escapes each character below U+0020 (`\u0001`, `\b`), and SBPL's string
/// reader does not document those escapes, so such a path could reach the
/// profile naming another file or fail to parse with an error that names no
/// path. Refusing it fails the caller with the path named.
pub(crate) fn quote(path: &Path) -> Result<String> {
    let text = path.to_string_lossy();
    if text.chars().any(|c| c < ' ') {
        bail!("cannot name {path:?} in a Seatbelt profile: the path holds a control character");
    }
    Ok(serde_json::to_string(&text)?)
}

/// The refusals [`profile`] documents; returns the writable sessions path.
fn validate(spec: &SeatbeltBoundarySpec) -> Result<PathBuf> {
    let named = [
        ("worktree", &spec.worktree),
        ("temp directory", &spec.tmp),
        ("Codex home", &spec.codex_home),
    ];
    for (what, path) in named {
        canonical_dir(what, path)?;
    }
    let sessions = spec.codex_home.join(SESSIONS);
    canonical_dir("Codex sessions directory", &sessions)?;
    for (index, (what, path)) in named.iter().enumerate() {
        for (other, other_path) in &named[index + 1..] {
            if overlaps(path, other_path) {
                bail!(
                    "the boundary's {what} {} and {other} {} overlap",
                    path.display(),
                    other_path.display()
                );
            }
        }
    }
    let writable = [&spec.worktree, &spec.tmp, &sessions];
    refuse_covering("protected", &spec.protected, &writable)?;
    refuse_covering("hidden", &spec.hidden, &writable)?;
    refuse_writable_socket(&spec.proxy_socket, &writable)?;
    let roots = spec
        .read_roots
        .iter()
        .chain(&spec.protected)
        .chain(&spec.hidden);
    for path in roots {
        anyhow::ensure!(path.is_absolute(), "{} is not absolute", path.display());
    }
    Ok(sessions)
}

/// Refuses a proxy socket that, in any spelling or at its target, lies below
/// a writable root: the worker could replace it with its own listener.
fn refuse_writable_socket(socket: &Path, writable: &[&PathBuf]) -> Result<()> {
    anyhow::ensure!(
        socket.is_absolute(),
        "the proxy socket {} is not absolute",
        socket.display()
    );
    let forms = with_targets(&[socket.to_path_buf()]);
    let spelled = forms.iter().flat_map(|form| spellings(form));
    for form in spelled {
        if let Some(root) = writable.iter().find(|root| form.starts_with(root)) {
            bail!(
                "the proxy socket {} lies below the writable {}",
                socket.display(),
                root.display()
            );
        }
    }
    Ok(())
}

/// `path` is an absolute directory reached without a symlink: a writable
/// root named through a link would let its target change under the rules.
fn canonical_dir(what: &str, path: &Path) -> Result<()> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("resolve the boundary's {what} {}", path.display()))?;
    if canonical != path {
        bail!(
            "the boundary's {what} {} is not canonical (it resolves to {}); a symlinked root is refused",
            path.display(),
            canonical.display()
        );
    }
    anyhow::ensure!(
        canonical.is_dir(),
        "the boundary's {what} {} is not a directory",
        path.display()
    );
    Ok(())
}

/// Refuses a `kind` path that, at its own spelling or its target, is or
/// contains a writable root: its deny would take that root away.
fn refuse_covering(kind: &str, paths: &[PathBuf], writable: &[&PathBuf]) -> Result<()> {
    for path in with_targets(paths) {
        if let Some(root) = writable.iter().find(|root| root.starts_with(&path)) {
            bail!(
                "the {kind} path {} covers the writable root {}",
                path.display(),
                root.display()
            );
        }
    }
    Ok(())
}

fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}
