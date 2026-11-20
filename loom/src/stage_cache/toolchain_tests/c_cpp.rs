//! C and C++: a compiler launcher's cache survives a fresh build directory,
//! and sccache's per-consumer server starts and serves hits inside the boundary.

use super::{output, Rig};

fn write_project(rig: &Rig) {
    rig.write(
        "CMakeLists.txt",
        "cmake_minimum_required(VERSION 3.10)\nproject(probe C)\nadd_executable(probe main.c)\n",
    );
    rig.write("main.c", "int main(void) { return 0; }\n");
}

/// Configures and builds into a build directory that does not exist yet.
fn build(launcher: &str, directory: &str) -> String {
    format!(
        "rm -rf {directory} && cmake -S . -B {directory} \
         -DCMAKE_C_COMPILER_LAUNCHER={launcher} && cmake --build {directory}"
    )
}

/// The direct and preprocessed hit counters of `ccache --print-stats`
/// (tab-separated `name<TAB>count` lines, ccache 4.4 and later).
fn ccache_hits(stats: &str) -> u64 {
    stats
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter(|(name, _)| matches!(*name, "direct_cache_hit" | "preprocessed_cache_hit"))
        .filter_map(|(_, count)| count.trim().parse::<u64>().ok())
        .sum()
}

/// The second build, in a different directory of the same tree, compiles
/// nothing ccache has not seen: `CCACHE_BASEDIR` makes the key path-independent.
#[test]
#[serial_test::serial]
fn ccache_hits_on_a_fresh_build_dir() {
    let test = "ccache_hits_on_a_fresh_build_dir";
    let Some(rig) = Rig::start(test, &["ccache", "cmake"]) else {
        return;
    };
    write_project(&rig);
    let second = format!("{} && ccache --print-stats", build("ccache", "build-b"));
    let (_, second) = rig.run_twice(&build("ccache", "build-a"), &second);
    let hits = ccache_hits(&second.stdout);
    assert!(hits > 0, "ccache reported no hit: {second:?}");
}

/// The hit count of `sccache --show-stats`: its `Cache hits` lines, whose last
/// field is a number (the `Cache hits rate` line ends in a percentage or `-`).
fn sccache_hits(stats: &str) -> u64 {
    stats
        .lines()
        .filter(|line| line.starts_with("Cache hits") && !line.contains("rate"))
        .filter_map(|line| line.split_whitespace().last()?.parse::<u64>().ok())
        .sum()
}

/// A CMake launcher named `sccache` builds confined: the host-job client
/// starts its own server at the stage socket, and the second build is served
/// from the disk cache the first one filled.
#[test]
#[serial_test::serial]
fn sccache_launcher_builds_confined() {
    let test = "sccache_launcher_builds_confined";
    let Some(rig) = Rig::start(test, &["sccache", "cmake"]) else {
        return;
    };
    write_project(&rig);
    let second = format!("{} && sccache --show-stats", build("sccache", "build"));
    let (_, second) = rig.run_twice(&build("sccache", "build"), &second);
    let hits = sccache_hits(&second.stdout);
    assert!(hits > 0, "sccache reported no hit: {}", output(&second));
}
