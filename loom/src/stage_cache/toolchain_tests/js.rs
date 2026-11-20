//! JavaScript: bun's and npm's caches are the stage's, and an install from a
//! local package repeats in a later command with `node_modules` gone.

use super::{output, Rig};

fn write_dependency(rig: &Rig) {
    rig.write(
        "dep/package.json",
        "{\"name\":\"local-dep\",\"version\":\"1.0.0\",\"main\":\"index.js\"}\n",
    );
    rig.write("dep/index.js", "module.exports = 41;\n");
}

fn write_app(rig: &Rig, dependency: &str) {
    rig.write(
        "package.json",
        &format!(
            "{{\"name\":\"app\",\"version\":\"1.0.0\",\"dependencies\":{{\"local-dep\":\"{dependency}\"}}}}\n"
        ),
    );
}

/// A frozen reinstall succeeds without `node_modules`, and bun's cache is the
/// stage's `bun` directory.
#[test]
#[serial_test::serial]
fn bun_cache_persists_across_commands() {
    let test = "bun_cache_persists_across_commands";
    let Some(rig) = Rig::start(test, &["bun"]) else {
        return;
    };
    write_dependency(&rig);
    write_app(&rig, "file:./dep");
    let (_, second) = rig.run_twice(
        "bun install",
        "rm -rf node_modules && bun install --frozen-lockfile && ls node_modules",
    );
    assert!(second.stdout.contains("local-dep"), "{second:?}");
    assert!(
        output(&second).contains("1 package installed"),
        "{second:?}"
    );
    assert!(
        rig.root().join("bun").is_dir(),
        "bun ignored its cache variable"
    );
}

/// An offline `npm ci` reinstalls the tarball without `node_modules`, and
/// npm's cache is the stage's `npm` directory.
#[test]
#[serial_test::serial]
fn npm_cache_persists_across_commands() {
    let test = "npm_cache_persists_across_commands";
    let Some(rig) = Rig::start(test, &["npm"]) else {
        return;
    };
    write_dependency(&rig);
    write_app(&rig, "file:./dep/local-dep-1.0.0.tgz");
    let (_, second) = rig.run_twice(
        "(cd dep && npm pack) && npm install ./dep/local-dep-1.0.0.tgz",
        "rm -rf node_modules && npm ci --offline && ls node_modules",
    );
    assert!(second.stdout.contains("local-dep"), "{second:?}");
    assert!(
        rig.root().join("npm/_cacache").is_dir(),
        "npm ignored its cache variable"
    );
}
