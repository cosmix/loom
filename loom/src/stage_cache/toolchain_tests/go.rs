//! Go: the build cache outlives a command, test results are never reused by a
//! host job, and the host's module cache seeds module resolution read-only.

use super::{listing, output, seed, Rig};
use std::path::PathBuf;
use std::process::Command;

fn write_module(rig: &Rig) {
    rig.write("go.mod", "module example.com/probe\n\ngo 1.21\n");
    rig.write(
        "probe.go",
        "package probe\n\nfunc Double(n int) int { return n * 2 }\n",
    );
    rig.write(
        "probe_test.go",
        "package probe\n\nimport \"testing\"\n\nfunc TestDouble(t *testing.T) {\n\
         \tif Double(2) != 4 {\n\t\tt.Fatal(\"bad\")\n\t}\n}\n",
    );
}

/// `go test -x` prints each tool invocation; a compile is `<tooldir>/compile -o ...`.
fn compiles(result: &crate::verify::criteria::CriterionResult) -> usize {
    output(result)
        .lines()
        .filter(|line| line.contains("/compile "))
        .count()
}

/// The standard library and the package compile in the first command and stay
/// in `GOCACHE`: the second command runs no compiler.
#[test]
#[serial_test::serial]
fn std_build_is_reused_by_the_second_command() {
    let test = "std_build_is_reused_by_the_second_command";
    let Some(rig) = Rig::start(test, &["go"]) else {
        return;
    };
    write_module(&rig);
    let (first, second) = rig.run_twice("go test -x ./...", "go test -x ./...");
    assert!(
        compiles(&first) > 0,
        "the cold build compiled nothing: {first:?}"
    );
    assert_eq!(compiles(&second), 0, "the warm build compiled: {second:?}");
}

/// A host job runs `go test` with `-count=1`, so a result a session cached in
/// the shared `GOCACHE` is never replayed as a pass.
#[test]
#[serial_test::serial]
fn test_results_are_never_reused_by_host_jobs() {
    let test = "test_results_are_never_reused_by_host_jobs";
    let Some(rig) = Rig::start(test, &["go"]) else {
        return;
    };
    write_module(&rig);
    let (_, second) = rig.run_twice("go test ./...", "go test ./...");
    let text = output(&second);
    let passed = text.contains("ok  \texample.com/probe");
    assert!(passed, "the test did not run and pass: {second:?}");
    assert!(
        !text.contains("(cached)"),
        "a result was reused: {second:?}"
    );
}

/// The host `GOMODCACHE` is the only module source (`GOPROXY` ends in `off`),
/// and resolving from it changes nothing there.
#[test]
#[serial_test::serial]
fn host_module_seed_resolves_offline() {
    let test = "host_module_seed_resolves_offline";
    let Some(coordinate) = seed(test, "LOOM_TEST_GO_SEED_MODULE") else {
        return;
    };
    let Some(rig) = Rig::start(test, &["go"]) else {
        return;
    };
    let (module, _) = coordinate
        .split_once('@')
        .expect("LOOM_TEST_GO_SEED_MODULE is module@version");
    let host = host_module_cache();
    let before = listing(&host);
    let script = format!(
        "go mod init example.com/seedprobe && GOSUMDB=off go get {coordinate} \
         && GOSUMDB=off go list -m {module}"
    );
    let result = rig.run(&script);
    assert!(
        result.success,
        "the seed did not resolve: {result:?}{}",
        rig.explain()
    );
    let version = coordinate.split_once('@').unwrap().1;
    assert!(
        result.stdout.contains(&format!("{module} {version}")),
        "{result:?}"
    );
    assert_eq!(before, listing(&host), "the host module cache changed");
}

fn host_module_cache() -> PathBuf {
    let output = Command::new("go")
        .args(["env", "GOMODCACHE"])
        .output()
        .expect("run go env");
    PathBuf::from(String::from_utf8_lossy(&output.stdout).trim())
}
