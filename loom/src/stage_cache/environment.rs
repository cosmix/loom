//! The per-ecosystem variables that point every tool's cache below the stage
//! cache root, layered over read-only host seeds where the tool supports it.

use super::roots::Seeds;
use super::writable::Writable;
use super::{Consumer, DeveloperTools, StageCache};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// `sizeof(sockaddr_un.sun_path)`: 104 bytes on macOS, 108 on Linux. A socket
/// path needs one byte for its NUL terminator, so it must be shorter.
const SUN_LEN: usize = if cfg!(target_os = "macos") { 104 } else { 108 };

/// Go's module proxy chain after the seed for sessions, workers and
/// provisioning, when the host names no `GOPROXY` of its own.
const DEFAULT_GOPROXY: &str = "https://proxy.golang.org,direct";

/// Host jobs rerun every test: a session can write `GOCACHE`, where Go
/// caches test results, so a planted entry would otherwise pass `go test`.
const GO_NO_TEST_CACHE: &str = "-count=1";

/// The host variables [`StageCache::environment`], [`super::seed_read_roots`]
/// and [`super::toolchain_read_roots`] read: seed locations, `HOME` and `PATH`,
/// plus the Go settings `GOPROXY` and `GOFLAGS`, which the consumers keep.
/// A `GOPROXY` carrying URL userinfo is a credential and is dropped
/// ([`host_env_from`]). `MAVEN_OPTS` is never read: it carries proxy
/// passwords. The daemon's environment keeps each name read here
/// (`daemon/server/environment.rs`).
pub(super) const HOST_ENV_NAMES: [&str; 9] = [
    "HOME",
    "PATH",
    "GOPATH",
    "GOMODCACHE",
    "CCACHE_DIR",
    "XDG_CACHE_HOME",
    "GRADLE_USER_HOME",
    "GOPROXY",
    "GOFLAGS",
];

/// Whether any entry of a `GOPROXY` list (separated by `,` or `|`) has userinfo
/// in its authority, the part between `scheme://` and the next `/`.
/// An entry without `://` is read as `https://` by Go, so its authority is the
/// text before the first `/`.
pub fn goproxy_has_userinfo(value: &OsStr) -> bool {
    let value = value.to_string_lossy();
    value.split([',', '|']).any(|entry| {
        let authority = entry.split_once("://").map_or(entry, |(_, rest)| rest);
        authority.split('/').next().unwrap_or("").contains('@')
    })
}

/// The `HOST_ENV_NAMES` of `vars`; a name that is not UTF-8 is skipped, and a
/// `GOPROXY` with URL userinfo is dropped so the default chain applies.
pub fn host_env_from<I>(vars: I) -> BTreeMap<String, OsString>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    vars.into_iter()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            let wanted = HOST_ENV_NAMES.contains(&name.as_str());
            let credential = name == "GOPROXY" && goproxy_has_userinfo(&value);
            (wanted && !credential).then_some((name, value))
        })
        .collect()
}

/// This process's `HOST_ENV_NAMES`: every consumer's seeds resolve from loom's
/// own environment, never from a confined command's filtered one.
pub fn host_env_from_process() -> BTreeMap<String, OsString> {
    host_env_from(std::env::vars_os())
}

/// `GOPROXY` for `consumer`: the host module seed, when there is one, ahead of
/// the host's `GOPROXY` (else `DEFAULT_GOPROXY`) for a session or worker, or
/// ahead of `off` for a host job. Provisioning fetches, so it takes the
/// `Session` chain. A seed reaching one of the `writable` roots is left out,
/// as [`super::seed_read_roots`] leaves it out of the read roots.
pub fn go_proxy(
    consumer: Consumer,
    host_env: &BTreeMap<String, OsString>,
    writable: &[&Path],
) -> OsString {
    let seed = Seeds::resolve(host_env, &Writable::new(writable)).go;
    proxy_chain(consumer, seed.as_deref(), host_env)
}

fn proxy_chain(
    consumer: Consumer,
    seed: Option<&Path>,
    host_env: &BTreeMap<String, OsString>,
) -> OsString {
    let mut proxy = OsString::new();
    if let Some(seed) = seed {
        proxy.push("file://");
        proxy.push(seed);
        proxy.push(",");
    }
    match consumer {
        Consumer::HostJob => proxy.push("off"),
        Consumer::Session | Consumer::Worker => match host_env.get("GOPROXY") {
            Some(host) if !host.is_empty() => proxy.push(host),
            _ => proxy.push(DEFAULT_GOPROXY),
        },
    }
    proxy
}

/// `GOFLAGS`: the host's, with `-count=1` added for a host job unless present.
fn go_flags(consumer: Consumer, host_env: &BTreeMap<String, OsString>) -> OsString {
    let mut flags = host_env.get("GOFLAGS").cloned().unwrap_or_default();
    if consumer == Consumer::HostJob {
        let present = flags
            .to_string_lossy()
            .split_whitespace()
            .any(|f| f == GO_NO_TEST_CACHE);
        if !present {
            if !flags.is_empty() {
                flags.push(" ");
            }
            flags.push(GO_NO_TEST_CACHE);
        }
    }
    flags
}

/// `GRADLE_OPTS` that keep a confined Gradle build in the client JVM. A forked
/// daemon talks to its client over loopback TCP, which the Seatbelt profile
/// denies. In Gradle 8.10 `BuildActionsFactory::canUseCurrentProcess` reuses
/// the client JVM for `--no-daemon` unless the client is a low-memory process
/// (`CurrentProcess::isLowMemoryProcess`: max heap <= 64 MB, which the start
/// script's `-Xmx64m` hits) or the build defines its own immutable JVM
/// arguments (`org.gradle.jvmargs`), which must then equal the client's
/// exactly. So this raises only the client heap (it follows the script's
/// `-Xmx64m`) and never sets `org.gradle.jvmargs`. The host's own `GRADLE_OPTS`
/// is never read: it can carry proxy passwords, like `MAVEN_OPTS`. A project
/// `gradle.properties` that sets `org.gradle.jvmargs` still forks.
const GRADLE_IN_PROCESS_OPTS: &str = "-Xmx1g";

/// Variables whose whole value is a directory below the root.
pub(super) const ROOT_DIRS: [(&str, &str); 13] = [
    ("GOCACHE", "go-build"),
    ("GOMODCACHE", "go-mod"),
    ("CCACHE_DIR", "ccache"),
    ("SCCACHE_DIR", "sccache"),
    ("GRADLE_USER_HOME", "gradle"),
    ("npm_config_cache", "npm"),
    ("npm_config_store_dir", "pnpm-store"),
    ("YARN_CACHE_FOLDER", "yarn"),
    ("BUN_INSTALL_CACHE_DIR", "bun"),
    ("DENO_DIR", "deno"),
    ("UV_CACHE_DIR", "uv"),
    ("PIP_CACHE_DIR", "pip"),
    ("XDG_CACHE_HOME", "xdg"),
];

/// Builds the environment, dropping any variable whose value would be empty.
struct Vars(BTreeMap<String, OsString>);

impl Vars {
    fn set(&mut self, name: &str, value: impl Into<OsString>) {
        let value = value.into();
        if !value.is_empty() {
            self.0.insert(name.to_string(), value);
        }
    }
}

impl StageCache {
    /// The variables `consumer` runs with: every cache directory below
    /// [`StageCache::root`], host seeds from `host_env` where they exist and
    /// reach neither the root nor `worktree_root` ([`super::seed_read_roots`]),
    /// and, with `developer`, the real macOS developer tools ahead of the host
    /// `PATH`.
    pub fn environment(
        &self,
        consumer: Consumer,
        worktree_root: &Path,
        host_env: &BTreeMap<String, OsString>,
        developer: Option<&DeveloperTools>,
    ) -> BTreeMap<String, OsString> {
        let mut vars = Vars(BTreeMap::new());
        let seeds = Seeds::resolve(host_env, &Writable::new(&[&self.root, worktree_root]));
        for (name, child) in ROOT_DIRS {
            vars.set(name, self.root.join(child));
        }
        vars.set(
            "GOPROXY",
            proxy_chain(consumer, seeds.go.as_deref(), host_env),
        );
        vars.set("GOFLAGS", go_flags(consumer, host_env));
        vars.set("CCACHE_BASEDIR", worktree_root);
        if let Some(seed) = &seeds.ccache {
            let mut storage = OsString::from("file:");
            storage.push(seed);
            storage.push("|read-only");
            vars.set("CCACHE_REMOTE_STORAGE", storage.clone());
            vars.set("CCACHE_SECONDARY_STORAGE", storage);
        }
        if let Some(socket) = self.sccache_socket(consumer) {
            vars.set("SCCACHE_SERVER_UDS", socket);
        }
        self.maven(&mut vars, seeds.maven.as_deref());
        if let Some(seed) = seeds.gradle {
            vars.set("GRADLE_RO_DEP_CACHE", seed);
        }
        vars.set("GRADLE_OPTS", GRADLE_IN_PROCESS_OPTS);
        if let Some(developer) = developer {
            developer_tools(&mut vars, developer, host_env.get("PATH"));
        }
        vars.0
    }

    /// The sccache server socket `consumer` starts, omitted when it would not
    /// fit `sun_path`. The consumers of one stage share writable caches, so
    /// the cache gives no integrity guarantee between them; separate sockets
    /// keep each consumer's compiler runs inside its own confinement, since an
    /// sccache server runs the compilers its clients ask for. A host job's
    /// socket is in the root, where Seatbelt grants `h.sock` alone and
    /// bubblewrap binds the root; a session's or worker's is in its private
    /// [`StageCache::with_socket_dir`], which no host job is granted.
    fn sccache_socket(&self, consumer: Consumer) -> Option<PathBuf> {
        let socket = match consumer {
            Consumer::HostJob => self.root.join("h.sock"),
            Consumer::Session => self.socket_dir.as_ref()?.join("s.sock"),
            Consumer::Worker => self.socket_dir.as_ref()?.join("w.sock"),
        };
        (socket.as_os_str().len() < SUN_LEN).then_some(socket)
    }

    /// `MAVEN_OPTS` naming only the stage's local repository and, with a
    /// seed, the read-only tail (Maven 3.9+). The host's own `MAVEN_OPTS` is
    /// never read: it can carry a proxy password.
    fn maven(&self, vars: &mut Vars, seed: Option<&Path>) {
        let mut opts = OsString::from("-Dmaven.repo.local=");
        opts.push(self.root.join("m2"));
        if let Some(seed) = seed {
            opts.push(" -Dmaven.repo.local.tail=");
            opts.push(seed);
        }
        vars.set("MAVEN_OPTS", opts);
    }
}

impl DeveloperTools {
    /// The real developer toolchain ahead of `host_path` on `PATH`, plus
    /// `SDKROOT` and `DEVELOPER_DIR`, for a confined command with or without
    /// a stage cache.
    pub fn environment(&self, host_path: Option<&OsString>) -> BTreeMap<String, OsString> {
        let mut vars = Vars(BTreeMap::new());
        developer_tools(&mut vars, self, host_path);
        vars.0
    }
}

/// `PATH` with the developer toolchain and `usr/bin` ahead of the host's,
/// plus `SDKROOT` and `DEVELOPER_DIR`, identical for every consumer.
fn developer_tools(vars: &mut Vars, developer: &DeveloperTools, host_path: Option<&OsString>) {
    let toolchain = developer
        .developer_dir
        .join("Toolchains/XcodeDefault.xctoolchain/usr/bin");
    let mut entries: Vec<PathBuf> = Vec::new();
    if toolchain.is_dir() {
        entries.push(toolchain);
    }
    entries.push(developer.developer_dir.join("usr/bin"));
    // A developer path containing the separator cannot go on PATH; the
    // host PATH then stays as it is.
    if let Ok(mut path) = std::env::join_paths(&entries) {
        if let Some(host) = host_path.filter(|host| !host.is_empty()) {
            path.push(":");
            path.push(host);
        }
        vars.set("PATH", path);
    }
    vars.set("SDKROOT", &developer.sdk_root);
    vars.set("DEVELOPER_DIR", &developer.developer_dir);
}
