//! Real uv and Bun against their configured cache lookup inside the confined
//! runner: a dependency present only in the warm host cache resolves offline, a
//! miss fails with the network denied, and the host cache never changes. Each
//! warm cache is built by the tool itself from a registry on the loopback.

use super::tool_tests::{install_tool, launcher_available, optional_tool};
use super::{wrap, NetworkAccess};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A loopback registry serving fixed paths until it is dropped.
struct Registry {
    port: u16,
    stop: Arc<AtomicBool>,
}

impl Registry {
    fn serve(routes: impl Fn(u16) -> HashMap<String, Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let routes = routes(port);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::Acquire) {
                    return;
                }
                if let Ok(stream) = stream {
                    respond(stream, &routes);
                }
            }
        });
        Self { port, stop }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn respond(mut stream: TcpStream, routes: &HashMap<String, Vec<u8>>) {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|end| end == b"\r\n\r\n") {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&chunk[..read]),
        }
    }
    let line = String::from_utf8_lossy(&request);
    let path = line.split_whitespace().nth(1).unwrap_or_default();
    let (status, body) = match routes.get(path) {
        Some(body) => ("200 OK", body.as_slice()),
        None => ("404 Not Found", &b""[..]),
    };
    let kind = match path {
        path if path.starts_with("/simple/") => "text/html",
        path if path.starts_with("/files/") || path.ends_with(".tgz") => "application/octet-stream",
        _ => "application/json",
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

/// `path`'s files and their bytes, for proving a host cache unchanged.
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_symlink() {
                let target = std::fs::read_link(entry.path()).unwrap();
                files.insert(entry.path(), target.into_os_string().into_encoded_bytes());
            } else {
                files.insert(entry.path(), std::fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

/// One tool installed in a working tree, with a home whose default cache it
/// warms and a home whose cache is empty.
struct Harness {
    root: tempfile::TempDir,
    tree: PathBuf,
    binary: PathBuf,
    warm_home: PathBuf,
    cold_home: PathBuf,
    cache: PathBuf,
}

impl Harness {
    fn new(tool: &Path, name: &str, cache: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let tree = root.path().join("tree");
        let warm_home = root.path().join("warm");
        let cold_home = root.path().join("cold");
        for directory in [&tree, &cold_home.join(cache), &warm_home.join(cache)] {
            std::fs::create_dir_all(directory).unwrap();
        }
        let binary = tree.join(name);
        install_tool(tool, &binary);
        let cache = warm_home.join(cache);
        Self {
            root,
            tree,
            binary,
            warm_home,
            cold_home,
            cache,
        }
    }

    /// Run the tool confined from `cwd`, through its configured cache lookup.
    /// `None` when this host cannot mount the overlay.
    fn confined(
        &self,
        cwd: &Path,
        home: &Path,
        args: &[&str],
        network: NetworkAccess,
    ) -> Option<Output> {
        let mut original = Command::new(&self.binary);
        original.current_dir(cwd).args(args);
        crate::process::apply_stage_environment(&mut original);
        for inherited in ["UV_CACHE_DIR", "BUN_INSTALL_CACHE_DIR", "XDG_CACHE_HOME"] {
            original.env_remove(inherited);
        }
        original.env("HOME", home).env("PATH", "/usr/bin:/bin");
        let (mut wrapped, scratch, _) = wrap(&original, Some(&self.tree), network).unwrap();
        if scratch.cache_overlays().is_empty() {
            return None;
        }
        scratch.populate().unwrap();
        let output = wrapped.output().unwrap();
        drop(scratch);
        Some(output)
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A wheel for `name`, built with the interpreter uv needs anyway.
fn wheel(directory: &Path, name: &str) -> Vec<u8> {
    let path = directory.join(format!("{name}.whl"));
    let script = r#"
import sys, zipfile
path, name = sys.argv[1:]
info = f"{name}-1.0.0.dist-info/"
with zipfile.ZipFile(path, "w") as wheel:
    wheel.writestr(f"{name}/__init__.py", "VALUE = 1\n")
    wheel.writestr(info + "METADATA", f"Metadata-Version: 2.1\nName: {name}\nVersion: 1.0.0\n")
    wheel.writestr(info + "WHEEL", "Wheel-Version: 1.0\nGenerator: fixture\nRoot-Is-Purelib: true\nTag: py3-none-any\n")
    wheel.writestr(info + "RECORD", "")
"#;
    let built = Command::new("python3")
        .args(["-I", "-c", script])
        .arg(&path)
        .arg(name)
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    std::fs::read(path).unwrap()
}

fn python_routes(directory: &Path) -> impl Fn(u16) -> HashMap<String, Vec<u8>> {
    let mut routes = HashMap::new();
    for (project, module) in [("warm-dep", "warm_dep"), ("cold-dep", "cold_dep")] {
        let file = format!("{module}-1.0.0-py3-none-any.whl");
        let link = format!("<a href=\"/files/{file}\">{file}</a>");
        routes.insert(format!("/simple/{project}/"), link.into_bytes());
        routes.insert(format!("/files/{file}"), wheel(directory, module));
    }
    move |_| routes.clone()
}

fn pyproject(directory: &Path, dependency: &str) {
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(
        directory.join("pyproject.toml"),
        format!(
            "[project]\nname = \"fixture\"\nversion = \"0.1.0\"\nrequires-python = \">=3.8\"\n\
             dependencies = [\"{dependency}\"]\n"
        ),
    )
    .unwrap();
}

/// Repeated and concurrent offline locks all find the package in the warm cache.
/// False when this host cannot mount the overlay.
fn uv_hits_the_warm_cache(harness: &Harness, offline: &[&str]) -> bool {
    let denied = NetworkAccess::Denied;
    for attempt in ["first", "second"] {
        let package = harness.tree.join(attempt);
        pyproject(&package, "warm-dep");
        let Some(output) = harness.confined(&package, &harness.warm_home, offline, denied) else {
            return false;
        };
        assert!(output.status.success(), "{attempt}: {}", stderr(&output));
        let lock = std::fs::read_to_string(package.join("uv.lock")).unwrap();
        assert!(lock.contains("name = \"warm-dep\""), "{lock}");
    }
    std::thread::scope(|scope| {
        for stage in ["stage-a", "stage-b"] {
            let package = harness.tree.join(stage);
            pyproject(&package, "warm-dep");
            scope.spawn(move || {
                let output = harness
                    .confined(&package, &harness.warm_home, offline, denied)
                    .unwrap();
                assert!(output.status.success(), "{stage}: {}", stderr(&output));
            });
        }
    });
    true
}

/// The registry is up and holds this one: only the denied network stops it.
fn a_uv_miss_needs_the_network(harness: &Harness, index: &str) {
    let missing = harness.tree.join("missing");
    pyproject(&missing, "cold-dep");
    let online = ["lock", "--index-url", index];
    let refused = harness
        .confined(&missing, &harness.warm_home, &online, NetworkAccess::Denied)
        .unwrap();
    assert!(
        !refused.status.success(),
        "a miss must not reach the network"
    );
    assert!(!missing.join("uv.lock").exists());
    let fetched = harness
        .confined(
            &missing,
            &harness.warm_home,
            &online,
            NetworkAccess::Provision,
        )
        .unwrap();
    assert!(fetched.status.success(), "{}", stderr(&fetched));
}

#[test]
fn uv_resolves_a_dependency_only_in_the_warm_host_cache_offline() {
    let name = "uv_resolves_a_dependency_only_in_the_warm_host_cache_offline";
    if !launcher_available(name) || which::which("python3").is_err() {
        return;
    }
    let Some(tool) = optional_tool("uv", "LOOM_TEST_UV_BIN") else {
        return;
    };
    let harness = Harness::new(&tool, "uv", ".cache/uv");
    let registry = Registry::serve(python_routes(harness.root.path()));
    let index = format!("{}/simple/", registry.url());
    let control = harness.root.path().join("control");
    pyproject(&control, "warm-dep");
    let warmed = Command::new(&harness.binary)
        .current_dir(&control)
        .args(["lock", "--index-url", &index, "--cache-dir"])
        .arg(&harness.cache)
        .output()
        .unwrap();
    assert!(warmed.status.success(), "{}", stderr(&warmed));
    let host_cache = snapshot(&harness.cache);

    let offline = ["lock", "--offline", "--index-url", &index];
    if !uv_hits_the_warm_cache(&harness, &offline) {
        eprintln!("{name}: skipped, this host cannot mount a cache overlay");
        return;
    }
    let denied = NetworkAccess::Denied;
    let cold = harness.tree.join("cold");
    pyproject(&cold, "warm-dep");
    let empty = harness
        .confined(&cold, &harness.cold_home, &offline, denied)
        .unwrap();
    assert!(!empty.status.success(), "an empty cache cannot resolve it");

    a_uv_miss_needs_the_network(&harness, &index);
    assert_eq!(
        snapshot(&harness.cache),
        host_cache,
        "the host cache must not change"
    );
}

#[path = "warm_cache_bun_tests.rs"]
mod bun;
