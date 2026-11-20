//! Python: uv's cache directory is the stage's and outlives a command, and a
//! wheel built in one command installs offline in the next.

use super::{output, Rig};
use std::path::Path;

/// A pure-Python wheel laid out by hand and zipped with the standard library,
/// so building it needs no build backend and no network.
const BUILD_WHEEL: &str = "mkdir -p dist \
    && (cd wheel && python3 -m zipfile -c ../dist/probe_pkg-0.1-py3-none-any.whl \
        probe_pkg probe_pkg-0.1.dist-info)";

fn write_wheel_sources(rig: &Rig) {
    let info = "wheel/probe_pkg-0.1.dist-info";
    rig.write("wheel/probe_pkg/__init__.py", "VALUE = 7\n");
    rig.write(
        &format!("{info}/METADATA"),
        "Metadata-Version: 2.1\nName: probe-pkg\nVersion: 0.1\n",
    );
    rig.write(
        &format!("{info}/WHEEL"),
        "Wheel-Version: 1.0\nGenerator: probe\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    );
    rig.write(
        &format!("{info}/RECORD"),
        "probe_pkg/__init__.py,,\nprobe_pkg-0.1.dist-info/METADATA,,\n\
         probe_pkg-0.1.dist-info/WHEEL,,\nprobe_pkg-0.1.dist-info/RECORD,,\n",
    );
}

#[test]
#[serial_test::serial]
fn uv_cache_persists_across_commands() {
    let test = "uv_cache_persists_across_commands";
    let Some(rig) = Rig::start(test, &["uv", "python3"]) else {
        return;
    };
    write_wheel_sources(&rig);
    let first = format!(
        "uv cache dir && mkdir -p \"$(uv cache dir)\" \
         && echo sentinel > \"$(uv cache dir)/sentinel\" && {BUILD_WHEEL}"
    );
    let second = "cat \"$(uv cache dir)/sentinel\" && uv venv .venv \
         && uv pip install --offline --no-index --find-links dist --python .venv probe-pkg \
         && .venv/bin/python -c 'import probe_pkg; print(probe_pkg.VALUE)'";
    let (first, second) = rig.run_twice(&first, second);
    let printed = first.stdout.lines().next().unwrap_or_default();
    let cache = Path::new(printed).canonicalize();
    assert_eq!(cache.ok(), Some(rig.root().join("uv")), "{first:?}");
    assert!(second.stdout.contains("sentinel"), "{second:?}");
    assert!(output(&second).contains("probe-pkg==0.1"), "{second:?}");
    assert_eq!(second.stdout.lines().last(), Some("7"), "{second:?}");
}
