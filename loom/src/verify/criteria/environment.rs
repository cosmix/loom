//! Output text that means the environment, not the test, failed.

/// Lowercase output fragments of a refused sandbox, network or disk.
const ENVIRONMENT_MARKERS: [&str; 9] = [
    "bwrap:",
    "operation not permitted",
    "temporary failure in name resolution",
    "could not resolve host",
    "couldn't resolve host",
    "network is unreachable",
    "connection refused",
    "read-only file system",
    "no space left on device",
];

/// The marker found in `stdout` or `stderr`, when the run failed for an
/// environment reason: a refused sandbox, a missing network or a full disk.
pub fn environment_marker(stdout: &str, stderr: &str) -> Option<&'static str> {
    let output = format!("{stderr}\n{stdout}").to_lowercase();
    ENVIRONMENT_MARKERS
        .into_iter()
        .find(|marker| output.contains(marker))
}
