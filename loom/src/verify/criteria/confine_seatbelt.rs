//! The macOS boundary: a Seatbelt profile run through `sandbox-exec`.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{ancestors, protected, read_roots_beside, writable_roots, CargoScratch};
use super::{environment_path, Grants, NetworkAccess};

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
/// roots, stage cache and the command's own read-only roots.
fn seatbelt_path_rules(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    grants: &Grants<'_>,
) -> Result<String> {
    let mut profile = String::new();
    let writable = writable_roots(tree, scratch, grants);
    let read_only = read_roots_beside(original, &writable, grants.seed_env().as_ref());
    let readable = writable.iter().map(|path| path.to_path_buf());
    let git_roots = grants
        .git
        .into_iter()
        .flat_map(|git| git.roots.iter().cloned());
    let readable: Vec<PathBuf> = read_only
        .iter()
        .cloned()
        .chain(readable)
        .chain(git_roots)
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
    for root in read_only.iter().filter(|p| *p != Path::new("/dev")) {
        profile.push_str(&format!(
            "(deny file-write* (subpath {}))",
            quote(&root.canonicalize()?)?
        ));
    }
    profile.push_str(&named_rules(original, tree, scratch, grants, &readable)?);
    Ok(profile)
}

/// Rules naming single paths, after the root rules they refine: the tree's
/// protected paths, files hidden above it and in the host's Cargo and Maven
/// homes, and the stage cache's host-job sccache socket.
fn named_rules(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    grants: &Grants<'_>,
    readable: &[PathBuf],
) -> Result<String> {
    let mut profile = String::new();
    let paths = protected::protected(tree);
    let named = [&paths.read_only, &paths.hidden_files, &paths.hidden_dirs];
    let named: Vec<PathBuf> = named.into_iter().flatten().cloned().collect();
    profile.push_str(&protected::seatbelt_rules(&paths, quote_checked(&named)?));
    if let Some(journals) = grants.memory_journals {
        profile.push_str(&protected::memory_journals_rule(&quote(&journals.dir)?));
    }
    let hidden = ancestors::hidden_above(tree, readable);
    profile.push_str(&ancestors::seatbelt_rules(&hidden, quote_checked(&hidden)?));
    if let Some(home) = scratch.host_cargo_home() {
        profile.push_str(&host_cargo_home_rules(home)?);
    }
    if let Some(home) = environment_path(original, "HOME") {
        profile.push_str(&hidden_rules(
            &HIDDEN_IN_M2.map(|name| home.join(".m2").join(name)),
        )?);
        let git = home.join(".config/git");
        profile.push_str(&readable_file_rules(
            &READABLE_IN_GIT_CONFIG.map(|name| git.join(name)),
        )?);
    }
    if let Some(root) = grants.cache_root {
        profile.push_str(&sccache_socket_rule(root)?);
    }
    profile.push_str(&scratch_socket_rule(scratch.path())?);
    Ok(profile)
}

/// Unix sockets below the command's own scratch directory (its `TMPDIR`),
/// bindable and connectable. loom creates that directory for this command
/// alone, so only processes of this boundary listen there. A tool that
/// starts a helper and waits on a socket it binds in `TMPDIR` needs it:
/// sccache's client binds `$TMPDIR/sccache*/sock` for its server's startup
/// notification and failed with `Operation not permitted` without it.
/// Seatbelt matches a socket's resolved path, so a symlink planted in
/// `TMPDIR` does not reach a host socket, and the profile refuses hard links
/// (`a_tmpdir_symlink_does_not_reach_a_host_socket`). Of the
/// [`ancestors::seatbelt_spellings`], only those holding the scratch path
/// as written are named, so `containment_policy_revision` can replace it.
fn scratch_socket_rule(scratch: &Path) -> Result<String> {
    let written = scratch.to_string_lossy();
    let mut rules = String::new();
    for spelling in ancestors::seatbelt_spellings(scratch) {
        if !spelling.to_string_lossy().contains(written.as_ref()) {
            continue;
        }
        let spelling = quote(&spelling)?;
        rules.push_str(&format!(
            "(allow network-bind network-outbound (local unix-socket (subpath {spelling})))\
             (allow network-outbound (remote unix-socket (subpath {spelling})))"
        ));
    }
    Ok(rules)
}

/// Maven files that never reach a confined command: the user's settings, which
/// can hold repository credentials, and the JDK toolchains file that
/// `actions/setup-java` writes. Maven fails on a file it can see but not read
/// (`Non-readable toolchains`), so they are hidden like the Cargo config files.
const HIDDEN_IN_M2: [&str; 3] = ["settings.xml", "settings-security.xml", "toolchains.xml"];

/// Git's per-user ignore and attributes files, which it and Cargo's Git
/// library read by default when no config names others (the user's config is
/// already out: `GIT_CONFIG_GLOBAL=/dev/null` in `apply_environment`). The
/// stage environment carries no `XDG_CONFIG_HOME`, so they sit under
/// `$HOME/.config/git`. Unreadable, the ignore file failed every Cargo build
/// script's first fingerprint with "Could not read repository exclude", and
/// hiding it does not help: the library fails on any open error other than
/// `ENOENT`, and Seatbelt denies with `EPERM`. They hold path patterns, no
/// credentials, so they are readable.
const READABLE_IN_GIT_CONFIG: [&str; 2] = ["ignore", "attributes"];

/// Rules letting a confined command read each existing file of `paths`, at its
/// path and at its symlink target.
fn readable_file_rules(paths: &[PathBuf]) -> Result<String> {
    let mut rules = String::new();
    for path in paths {
        let Ok(target) = path.canonicalize() else {
            continue;
        };
        for file in std::iter::once(path.clone()).chain((target != *path).then_some(target)) {
            rules.push_str(&format!("(allow file-read* (literal {}))", quote(&file)?));
        }
    }
    Ok(rules)
}

/// The host-job sccache server socket in the stage cache root
/// (`SCCACHE_SERVER_UDS`), the only Unix socket a confined command may bind or
/// connect to: the session's and the workers' servers run compilers outside
/// this boundary. The variable names the canonical root, which a process can
/// also reach through a `/private` or firmlink spelling, so each spelling gets
/// the rule.
fn sccache_socket_rule(root: &Path) -> Result<String> {
    let mut rules = String::new();
    for socket in ancestors::seatbelt_spellings(&root.join("h.sock")) {
        let socket = quote(&socket)?;
        rules.push_str(&format!(
            "(allow network-bind network-outbound (local unix-socket (path-literal {socket})))\
             (allow network-outbound (remote unix-socket (path-literal {socket})))"
        ));
    }
    Ok(rules)
}

/// IP networking every confined command gets: binding and receiving on any
/// local address, and connecting or sending only to this host. Gradle needs
/// it in-process: each cache lock binds a UDP socket on the wildcard address
/// (`FileLockCommunicator`) and pings lock owners over loopback UDP, and its
/// test and compiler workers connect back to the build over loopback TCP.
///
/// Seatbelt, as measured on macOS 27, matches a `local` filter against every
/// local address alike (`"localhost:*"` and `"*:*"` admit wildcard and LAN
/// binds the same way), and `network-inbound` sees no peer address, so bind,
/// listen and receive cannot be held to loopback; `"*:*"` says what they
/// allow. `network-outbound` with `(remote ip "localhost:*")` admits only the
/// host's own configured addresses (loopback, its aliases and the host's LAN
/// addresses, never an unconfigured `127.0.0.2`), checked on every `connect`
/// and `sendto`. So nothing leaves for another host, while a peer on the
/// network can reach a socket a command binds on the wildcard, and every
/// host service listening on loopback is reachable (residual:
/// `concerns/stage-build-cache-gaps.md`). The exception is sccache's default
/// TCP port: a host sccache server there runs compilers outside this
/// boundary, and a confined client falls back to it whenever
/// `SCCACHE_SERVER_UDS` is omitted. Pinned by
/// `confine_seatbelt_network_tests.rs`.
const NETWORK_RULES: &str = "(allow network-bind network-inbound (local ip \"*:*\"))\
     (allow network-outbound (remote ip \"localhost:*\"))\
     (deny network-outbound (remote tcp \"localhost:4226\"))";

/// Provisioning reaches the network, DNS included (the resolver's
/// `mDNSResponder` socket), but no stage's cache root and no session's scratch
/// directory, where the stage cache's and the sessions' sccache servers
/// listen: they run compilers outside this boundary. These denials precede
/// the path rules, so [`sccache_socket_rule`]'s later grant of the host-job
/// socket still wins.
fn provision_network_rules(grants: &Grants<'_>) -> Result<String> {
    let mut rules = String::from("(allow network-outbound)");
    let stage_roots = grants.cache_root.and_then(Path::parent);
    let scratch = crate::relay::scratch_root_from_env().ok();
    for base in stage_roots
        .map(Path::to_path_buf)
        .into_iter()
        .chain(scratch)
    {
        let canonical = base.canonicalize().ok().filter(|path| *path != base);
        for spelling in std::iter::once(base).chain(canonical) {
            for path in ancestors::seatbelt_spellings(&spelling) {
                let prefix = regex_prefix(&path)?;
                rules.push_str(&format!(
                    "(deny network-outbound (remote unix-socket (path-regex #\"{prefix}\")))"
                ));
            }
        }
    }
    Ok(rules)
}

/// An SBPL regex literal body matching every path below `directory`. A
/// regex literal has no escape for `"`, so a path holding one (or a control
/// character) is refused, naming it.
fn regex_prefix(directory: &Path) -> Result<String> {
    quote(directory)?;
    let text = directory.to_string_lossy();
    if text.contains('"') {
        anyhow::bail!(
            "confined verification cannot name {directory:?} in a Seatbelt profile: \
             the path holds a double quote"
        );
    }
    let mut regex = String::from("^");
    for character in text.trim_end_matches('/').chars() {
        if "\\.^$|?*+()[]{}".contains(character) {
            regex.push('\\');
        }
        regex.push(character);
    }
    regex.push('/');
    Ok(regex)
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
    rules.push_str(&hidden_rules(
        &HIDDEN_IN_CARGO_HOME.map(|name| home.join(name)),
    )?);
    Ok(rules)
}

/// Rules hiding each existing file of `paths`, metadata included, at its path
/// and at its symlink target.
fn hidden_rules(paths: &[PathBuf]) -> Result<String> {
    let mut hidden = Vec::new();
    for path in paths {
        if path.symlink_metadata().is_err() {
            continue;
        }
        match path.canonicalize() {
            Ok(target) if &target != path => hidden.push(target),
            _ => {}
        }
        hidden.push(path.clone());
    }
    Ok(ancestors::seatbelt_rules(&hidden, quote_checked(&hidden)?))
}

pub(super) fn platform_command(
    original: &Command,
    tree: &Path,
    // Seatbelt keeps the process's own working directory, set by `wrap`.
    _cwd: &Path,
    program: &Path,
    scratch: &CargoScratch,
    network: NetworkAccess,
    grants: &Grants<'_>,
) -> Result<Command> {
    let launcher = Path::new("/usr/bin/sandbox-exec");
    if !launcher.is_file() {
        anyhow::bail!("confined verification requires sandbox-exec");
    }
    // dyld opens the root directory while discovering the system Cryptex cache.
    // `process*` grants no signals: `same-sandbox` lets the command stop its own
    // children and groups, and nothing started outside this sandbox instance.
    let mut profile = String::from("(version 1)(deny default)(allow process*)(allow signal (target same-sandbox))(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal \"/\"))");
    profile.push_str(NETWORK_RULES);
    if network == NetworkAccess::Provision {
        profile.push_str(&provision_network_rules(grants)?);
    }
    profile.push_str(&seatbelt_path_rules(original, tree, scratch, grants)?);
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

#[cfg(test)]
#[path = "confine_seatbelt_network_tests.rs"]
mod network_tests;

#[cfg(test)]
#[path = "confine_provision_tests.rs"]
mod provision_tests;

#[cfg(test)]
#[path = "confine_seatbelt_signal_tests.rs"]
mod signal_tests;

#[cfg(test)]
#[path = "confine_git_config_tests.rs"]
mod git_config_tests;
