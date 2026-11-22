//! The macOS boundary: a Seatbelt profile run through `sandbox-exec`. The
//! profile renders on any host ([`profile`]), so its policy contracts run on
//! Linux too; only macOS applies it.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::stage_cache::base_checkout::BASE_CHECKOUT_PARENTS;
use crate::stage_cache::StageCache;

use super::{ancestors, protected, read_roots_beside, writable_roots, CargoScratch};
use super::{environment_path, Grants, NetworkAccess};

use super::seatbelt_files::{git_config_rules, hidden_rules, host_cargo_home_rules};
// SBPL quoting is shared with the native worker's macOS boundary; it
// refuses a control character (see its docs for why).
use crate::native_worker::boundary::seatbelt_profile::quote;
#[cfg(target_os = "macos")]
use crate::native_worker::boundary::seatbelt_profile::quote_checked;

/// Maven files that never reach a confined command: the user's settings, which
/// can hold repository credentials, and the JDK toolchains file that
/// `actions/setup-java` writes. Maven fails on a file it can see but not read
/// (`Non-readable toolchains`), so they are hidden like the Cargo config files.
const HIDDEN_IN_M2: [&str; 3] = ["settings.xml", "settings-security.xml", "toolchains.xml"];

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
/// protected paths, the directories above it listed, the Cargo files there
/// hidden and each `package.json` there readable, files hidden in the host's Cargo and Maven homes, and the stage
/// cache's host-job sccache socket.
fn named_rules(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    grants: &Grants<'_>,
    readable: &[PathBuf],
) -> Result<String> {
    let mut profile = protected_rules(tree, grants)?;
    profile.push_str(&ancestors::rules_above(tree, readable)?);
    if let Some(home) = host_cargo_home(scratch) {
        profile.push_str(&host_cargo_home_rules(home)?);
    }
    if let Some(home) = environment_path(original, "HOME") {
        profile.push_str(&hidden_rules(
            &HIDDEN_IN_M2.map(|name| home.join(".m2").join(name)),
        )?);
        profile.push_str(&git_config_rules(&home)?);
    }
    if let Some(root) = grants.cache_root {
        profile.push_str(&sccache_socket_rule(root)?);
    }
    profile.push_str(&scratch_socket_rule(scratch.path())?);
    Ok(profile)
}

/// The tree's protected paths and the memory journals' read-only grant.
#[cfg(target_os = "macos")]
fn protected_rules(tree: &Path, grants: &Grants<'_>) -> Result<String> {
    let paths = protected::protected(tree);
    let named = [&paths.read_only, &paths.hidden_files, &paths.hidden_dirs];
    let named: Vec<PathBuf> = named.into_iter().flatten().cloned().collect();
    let mut rules = protected::seatbelt_rules(&paths, quote_checked(&named)?);
    if let Some(journals) = grants.memory_journals {
        rules.push_str(&protected::memory_journals_rule(&quote(&journals.dir)?));
    }
    Ok(rules)
}

/// Off macOS the profile renders only for contract tests, and the protected
/// paths' Seatbelt rules are not built there: a tree that has some, or a
/// journals grant, is refused instead of rendered without them.
#[cfg(not(target_os = "macos"))]
fn protected_rules(tree: &Path, grants: &Grants<'_>) -> Result<String> {
    let none = protected::protected(tree) == protected::Protected::default();
    anyhow::ensure!(
        none && grants.memory_journals.is_none(),
        "the Seatbelt profile renders protected paths only on macOS"
    );
    Ok(String::new())
}

/// The host Cargo home the command reads in place.
#[cfg(target_os = "macos")]
fn host_cargo_home(scratch: &CargoScratch) -> Option<&Path> {
    scratch.host_cargo_home()
}

/// Off macOS a scratch names no home read in place (bubblewrap mounts a
/// private one), so a contract-test profile carries no Cargo home rules.
#[cfg(not(target_os = "macos"))]
fn host_cargo_home(_: &CargoScratch) -> Option<&Path> {
    None
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

/// The directories below this process's cache base holding other consumers'
/// sccache servers: `<cache base>/loom/stage-cache`, every stage's cache root,
/// and `<cache base>/loom/stage-base`, every base run's cache. Derived as
/// `StageCache::resolve_from_process` derives a root; the repository and stage
/// named here only pick a root whose parent is wanted. A missing `HOME` fails
/// the profile: provisioning without these denials would reach those servers.
fn process_cache_bases() -> Result<Vec<PathBuf>> {
    let cache = StageCache::resolve_from_process(Path::new("/"), "provision")?;
    let stage_caches = cache
        .root()
        .parent()
        .context("a stage cache root has a parent")?;
    let base_runs = stage_caches.with_file_name(BASE_CHECKOUT_PARENTS[1]);
    Ok(vec![stage_caches.to_path_buf(), base_runs])
}

/// Provisioning reaches the network, DNS included (the resolver's
/// `mDNSResponder` socket), but no Unix socket below [`process_cache_bases`],
/// the granted cache root's parent or the session scratch base, where the
/// stage caches', base runs' and sessions' sccache servers listen: they run
/// compilers outside this boundary. The granted root's parent alone would miss
/// them for a cache outside `<cache base>/loom/stage-cache` (a base run's, or
/// the impact base comparison's temporary one). These denials precede the
/// path rules, so [`sccache_socket_rule`]'s later grant of the granted root's
/// own `h.sock` still wins.
fn provision_network_rules(grants: &Grants<'_>) -> Result<String> {
    let mut rules = String::from("(allow network-outbound)");
    let mut bases = process_cache_bases()?;
    let granted = grants.cache_root.and_then(Path::parent);
    let scratch = crate::relay::scratch_root_from_env().ok();
    for base in granted.map(Path::to_path_buf).into_iter().chain(scratch) {
        if !bases.contains(&base) {
            bases.push(base);
        }
    }
    for base in bases {
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

/// The rules every profile starts with. dyld opens the root directory while
/// discovering the system Cryptex cache, hence the literal `/`.
const BASE_RULES: &str = "(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal \"/\"))";

/// `process*` grants no signals. `same-sandbox` lets a command stop its own
/// children and process groups; an unqualified `(allow signal)` would let it
/// signal the daemon, the operator's shell and every other process of the
/// same user.
const SIGNAL_RULE: &str = "(allow signal (target same-sandbox))";

/// IP networking of a stage that sets `sandbox.network.allow_local_binding`
/// ([`NetworkAccess::Loopback`]): servers and tests may listen and dial on this
/// host. Every other confined command gets no IP networking.
///
/// Measured residual (macOS 27.2.1, real `sandbox-exec`): a `local` filter
/// cannot hold a bind to loopback, since `"localhost"` matches every local
/// address and `"127.0.0.1:*"` is a parse error. A command can therefore
/// bind `0.0.0.0` or a LAN address, and a peer on the network can then reach
/// its listener. The `localhost:*` outbound grant also reaches every listener on the
/// host's loopback (the loom dashboard, databases, dev servers), and a command
/// can take a port a host client expects, such as an OAuth callback. A connect
/// to an address off this host stays denied (1.1.1.1:443). Bubblewrap's private
/// network namespace needs no such rule. Revisit if Seatbelt gains a
/// loopback-only bind filter.
///
/// The outbound grant is two rules, `tcp` then `udp`, so [`SCCACHE_PORT_DENY`]
/// can override it. Measured with real `sandbox-exec`: after a single
/// `(remote ip "localhost:*")` allow, no port deny holds on macOS 26.7 (the
/// `macos-26-arm64` CI image) in any spelling or order; after the split pair a
/// `tcp` deny refuses its port on 26.7 and on 27.2.1, over IPv4 and IPv6.
const LOOPBACK_NETWORK: &str = "(allow network-bind network-inbound (local ip \"localhost:*\"))\
     (allow network-outbound (remote tcp \"localhost:*\"))\
     (allow network-outbound (remote udp \"localhost:*\"))";

/// sccache's default TCP port. A host sccache server there runs compilers
/// outside this boundary, and a confined sccache client falls back to it
/// whenever `SCCACHE_SERVER_UDS` is unset, so a loopback grant must not reach
/// it. The deny follows [`LOOPBACK_NETWORK`]: Seatbelt applies the last matching
/// rule (measured: a host listener on 4226 is refused, another port succeeds).
const SCCACHE_PORT_DENY: &str = "(deny network-outbound (remote tcp \"localhost:4226\"))";

/// The Seatbelt profile confining `original` to the canonical `tree` with
/// `scratch` and `grants`. It starts nothing and needs no launcher, so tests
/// on every host render the profile [`platform_command`] applies.
pub(super) fn profile(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    network: NetworkAccess,
    grants: &Grants<'_>,
) -> Result<String> {
    let mut profile = format!("{BASE_RULES}{SIGNAL_RULE}");
    match network {
        NetworkAccess::Denied => {}
        NetworkAccess::Loopback => {
            profile.push_str(&format!("{LOOPBACK_NETWORK}{SCCACHE_PORT_DENY}"))
        }
        NetworkAccess::Provision => profile.push_str(&provision_network_rules(grants)?),
    }
    profile.push_str(&seatbelt_path_rules(original, tree, scratch, grants)?);
    Ok(profile)
}

/// The profile a confined `original` in the canonical `tree` runs under with
/// no grant beyond its own scratch, for contract tests on any host.
#[cfg(test)]
pub(crate) fn seatbelt_profile(
    original: &Command,
    tree: &Path,
    network: NetworkAccess,
) -> Result<String> {
    let scratch = CargoScratch::prepare(original, tree, false, false)?;
    profile(original, tree, &scratch, network, &Grants::default())
}

#[cfg(target_os = "macos")]
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
    let profile = profile(original, tree, scratch, network, grants)?;
    let mut command = Command::new(launcher);
    command
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(original.get_args());
    Ok(command)
}

#[cfg(test)]
#[path = "confine_seatbelt_network_tests.rs"]
mod network_tests;

#[cfg(all(test, target_os = "macos"))]
#[path = "confine_seatbelt_tests.rs"]
mod tests;

#[cfg(all(test, target_os = "macos"))]
#[path = "confine_provision_tests.rs"]
mod provision_tests;

#[cfg(all(test, target_os = "macos"))]
#[path = "confine_seatbelt_signal_tests.rs"]
mod signal_tests;

#[cfg(all(test, target_os = "macos"))]
#[path = "confine_git_config_tests.rs"]
mod git_config_tests;
