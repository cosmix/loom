//! Real Bun against its configured cache lookup in the confined runner.

use super::*;

/// A published npm package holding one file, as `.tgz` bytes.
fn tarball(directory: &Path, name: &str) -> Vec<u8> {
    let source = directory.join(name).join("package");
    std::fs::create_dir_all(&source).unwrap();
    let manifest = format!("{{\"name\":\"{name}\",\"version\":\"1.0.0\",\"main\":\"index.js\"}}");
    std::fs::write(source.join("package.json"), manifest).unwrap();
    std::fs::write(source.join("index.js"), "module.exports = 1;\n").unwrap();
    let archive = directory.join(format!("{name}.tgz"));
    let built = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(directory.join(name))
        .arg("package")
        .output()
        .unwrap();
    assert!(built.status.success(), "{}", stderr(&built));
    std::fs::read(archive).unwrap()
}

fn npm_routes(directory: &Path) -> impl Fn(u16) -> HashMap<String, Vec<u8>> {
    let archives: Vec<(&str, Vec<u8>)> = ["warm-dep", "cold-dep"]
        .into_iter()
        .map(|name| (name, tarball(directory, name)))
        .collect();
    move |port| {
        let mut routes = HashMap::new();
        for (name, archive) in &archives {
            let path = format!("/{name}/-/{name}-1.0.0.tgz");
            let manifest = format!(
                "{{\"name\":\"{name}\",\"dist-tags\":{{\"latest\":\"1.0.0\"}},\"versions\":\
                 {{\"1.0.0\":{{\"name\":\"{name}\",\"version\":\"1.0.0\",\"main\":\"index.js\",\
                 \"dist\":{{\"tarball\":\"http://127.0.0.1:{port}{path}\"}}}}}}}}"
            );
            routes.insert(format!("/{name}"), manifest.into_bytes());
            routes.insert(path, archive.clone());
        }
        routes
    }
}

/// A package depending on `dependency`, locked through `registry` with a real
/// install that fills `cache`.
fn locked_package(bun: &Path, directory: &Path, dependency: &str, registry: &str, cache: &Path) {
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(
        directory.join("package.json"),
        format!("{{\"name\":\"fixture\",\"version\":\"0.1.0\",\"dependencies\":{{\"{dependency}\":\"1.0.0\"}}}}"),
    )
    .unwrap();
    std::fs::write(
        directory.join("bunfig.toml"),
        format!("[install]\nregistry = \"{registry}/\"\n"),
    )
    .unwrap();
    let installed = Command::new(bun)
        .current_dir(directory)
        .arg("install")
        .env("BUN_INSTALL_CACHE_DIR", cache)
        .env("HOME", directory)
        .output()
        .unwrap();
    assert!(installed.status.success(), "{}", stderr(&installed));
    std::fs::remove_dir_all(directory.join("node_modules")).unwrap();
}

const FROZEN: [&str; 2] = ["install", "--frozen-lockfile"];

/// Repeated and concurrent offline installs all find the package in the warm
/// cache. False when this host cannot mount the overlay.
fn bun_hits_the_warm_cache(harness: &Harness, warm: &Path) -> bool {
    let denied = NetworkAccess::Denied;
    for attempt in ["first", "second"] {
        let Some(output) = harness.confined(warm, &harness.warm_home, &FROZEN, denied) else {
            return false;
        };
        assert!(output.status.success(), "{attempt}: {}", stderr(&output));
        assert!(warm.join("node_modules/warm-dep/index.js").is_file());
        std::fs::remove_dir_all(warm.join("node_modules")).unwrap();
    }
    std::thread::scope(|scope| {
        for stage in ["stage-a", "stage-b"] {
            let package = harness.tree.join(stage);
            std::fs::create_dir(&package).unwrap();
            for file in ["package.json", "bunfig.toml", "bun.lock"] {
                std::fs::copy(warm.join(file), package.join(file)).unwrap();
            }
            scope.spawn(move || {
                let output = harness
                    .confined(&package, &harness.warm_home, &FROZEN, denied)
                    .unwrap();
                assert!(output.status.success(), "{stage}: {}", stderr(&output));
                assert!(package.join("node_modules/warm-dep/index.js").is_file());
            });
        }
    });
    true
}

/// The registry is up and holds this one: only the denied network stops it.
fn a_bun_miss_needs_the_network(harness: &Harness, missing: &Path) {
    let refused = harness
        .confined(missing, &harness.warm_home, &FROZEN, NetworkAccess::Denied)
        .unwrap();
    assert!(
        !refused.status.success(),
        "a miss must not reach the network"
    );
    assert!(!missing.join("node_modules/cold-dep").exists());
    let fetched = harness
        .confined(
            missing,
            &harness.warm_home,
            &FROZEN,
            NetworkAccess::Provision,
        )
        .unwrap();
    assert!(fetched.status.success(), "{}", stderr(&fetched));
    assert!(missing.join("node_modules/cold-dep/index.js").is_file());
}

#[test]
fn bun_installs_a_dependency_only_in_the_warm_host_cache_offline() {
    let name = "bun_installs_a_dependency_only_in_the_warm_host_cache_offline";
    if !launcher_available(name) {
        return;
    }
    let Some(tool) = optional_tool("bun", "LOOM_TEST_BUN_BIN") else {
        return;
    };
    let harness = Harness::new(&tool, "bun", ".bun/install/cache");
    let registry = Registry::serve(npm_routes(harness.root.path()));
    let url = registry.url();
    let warm = harness.tree.join("warm");
    locked_package(&harness.binary, &warm, "warm-dep", &url, &harness.cache);
    let missing = harness.tree.join("missing");
    let elsewhere = harness.root.path().join("elsewhere-cache");
    locked_package(&harness.binary, &missing, "cold-dep", &url, &elsewhere);
    let host_cache = snapshot(&harness.cache);

    if !bun_hits_the_warm_cache(&harness, &warm) {
        eprintln!("{name}: skipped, this host cannot mount a cache overlay");
        return;
    }
    let denied = NetworkAccess::Denied;
    let empty = harness
        .confined(&warm, &harness.cold_home, &FROZEN, denied)
        .unwrap();
    assert!(!empty.status.success(), "an empty cache cannot resolve it");

    a_bun_miss_needs_the_network(&harness, &missing);
    assert_eq!(
        snapshot(&harness.cache),
        host_cache,
        "the host cache must not change"
    );
}
