//! The per-ecosystem variables that point every tool's cache below the stage
//! cache root, layered over read-only host seeds where the tool supports it.

use super::roots::Seeds;
use super::{Consumer, DeveloperTools, StageCache};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// `sizeof(sockaddr_un.sun_path)`: 104 bytes on macOS, 108 on Linux. A socket
/// path needs one byte for its NUL terminator, so it must be shorter.
const SUN_LEN: usize = if cfg!(target_os = "macos") { 104 } else { 108 };

/// Go's module proxy chain after the seed when the host names none.
const DEFAULT_GOPROXY: &str = "https://proxy.golang.org,direct";

/// Host jobs rerun every test: a session can write `GOCACHE`, where Go
/// caches test results, so a planted entry would otherwise pass `go test`.
const GO_NO_TEST_CACHE: &str = "-count=1";

/// Variables whose whole value is a directory below the root.
const ROOT_DIRS: [(&str, &str); 13] = [
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
    /// [`StageCache::root`], host seeds from `host_env` where they exist, and,
    /// with `developer`, the real macOS developer tools ahead of the host `PATH`.
    pub fn environment(
        &self,
        consumer: Consumer,
        worktree_root: &Path,
        host_env: &BTreeMap<String, OsString>,
        developer: Option<&DeveloperTools>,
    ) -> BTreeMap<String, OsString> {
        let mut vars = Vars(BTreeMap::new());
        let seeds = Seeds::resolve(host_env);
        for (name, child) in ROOT_DIRS {
            vars.set(name, self.root.join(child));
        }
        self.go(&mut vars, consumer, host_env, seeds.go.as_deref());
        vars.set("CCACHE_BASEDIR", worktree_root);
        if let Some(seed) = &seeds.ccache {
            let mut storage = OsString::from("file:");
            storage.push(seed);
            storage.push("|read-only");
            vars.set("CCACHE_REMOTE_STORAGE", storage.clone());
            vars.set("CCACHE_SECONDARY_STORAGE", storage);
        }
        let socket = self.root.join(socket_name(consumer));
        if socket.as_os_str().len() < SUN_LEN {
            vars.set("SCCACHE_SERVER_UDS", socket);
        }
        self.maven(&mut vars, host_env, seeds.maven.as_deref());
        if let Some(seed) = seeds.gradle {
            vars.set("GRADLE_RO_DEP_CACHE", seed);
        }
        if let Some(developer) = developer {
            developer_tools(&mut vars, developer, host_env.get("PATH"));
        }
        vars.0
    }

    fn go(
        &self,
        vars: &mut Vars,
        consumer: Consumer,
        host_env: &BTreeMap<String, OsString>,
        seed: Option<&Path>,
    ) {
        let tail = match consumer {
            Consumer::HostJob => OsString::from("off"),
            Consumer::Session | Consumer::Worker => host_env
                .get("GOPROXY")
                .filter(|proxy| !proxy.is_empty())
                .cloned()
                .unwrap_or_else(|| DEFAULT_GOPROXY.into()),
        };
        let mut proxy = OsString::new();
        if let Some(seed) = seed {
            proxy.push("file://");
            proxy.push(seed);
            proxy.push(",");
        }
        proxy.push(tail);
        vars.set("GOPROXY", proxy);
        if consumer == Consumer::HostJob {
            let host = host_env.get("GOFLAGS").map(OsString::as_os_str);
            vars.set("GOFLAGS", appended(host, GO_NO_TEST_CACHE, true));
        }
    }

    fn maven(&self, vars: &mut Vars, host_env: &BTreeMap<String, OsString>, seed: Option<&Path>) {
        let mut local = OsString::from("-Dmaven.repo.local=");
        local.push(self.root.join("m2"));
        let host = host_env.get("MAVEN_OPTS").map(OsString::as_os_str);
        let mut opts = appended(host, &local, false);
        if let Some(seed) = seed {
            let mut tail = OsString::from("-Dmaven.repo.local.tail=");
            tail.push(seed);
            opts = appended(Some(&opts), &tail, false);
        }
        vars.set("MAVEN_OPTS", opts);
    }
}

/// The sccache server socket each consumer starts: a host job reaching the
/// session's server would have it run compilers outside the confinement.
fn socket_name(consumer: Consumer) -> &'static str {
    match consumer {
        Consumer::Session => "s.sock",
        Consumer::Worker => "w.sock",
        Consumer::HostJob => "h.sock",
    }
}

/// `host` and `option` joined by a space; with `dedup`, `host` unchanged when
/// one of its space-separated words already is `option`.
fn appended(host: Option<&OsStr>, option: impl AsRef<OsStr>, dedup: bool) -> OsString {
    let option = option.as_ref();
    let host = host.filter(|host| !host.is_empty());
    let Some(host) = host else {
        return option.to_os_string();
    };
    let present = host
        .as_encoded_bytes()
        .split(|byte| *byte == b' ')
        .any(|word| word == option.as_encoded_bytes());
    let mut joined = host.to_os_string();
    if !(dedup && present) {
        joined.push(" ");
        joined.push(option);
    }
    joined
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
