//! Guard: only `helpers.rs` may spawn the loom test binary directly.
//!
//! The binary these tests spawn is not built with `cfg(test)`, so it reads
//! its `LOOM_*` session variables straight from the real process environment.
//! `helpers::loom_cmd()` scrubs those before every spawn (see its doc
//! comment); a raw call to `Command::new` around `env!("CARGO_BIN_EXE_loom")`
//! elsewhere skips that scrub and would inherit whatever `LOOM_*` identity the
//! `cargo test` process itself happens to be running under — including this
//! very suite's own loom session.

use std::fs;
use std::path::{Path, PathBuf};

/// Substrings (whitespace stripped, see [`strip_whitespace`]) that spawn the
/// loom binary directly, bypassing `helpers::loom_cmd()`'s environment scrub.
/// Passing the binary's path as an env var for a hook script to read later
/// (e.g. naming the binary's own env-var handle in a `.env(...)` call) is not
/// a direct spawn and does not match either pattern.
///
/// Built from parts at runtime rather than as string literals: a literal
/// copy of either needle would make this very file — which the test below
/// also scans — match itself.
fn forbidden_patterns() -> Vec<String> {
    let cargo_bin_exe_loom = ["CARGO_BIN_", "EXE_loom"].concat();
    vec![
        ["Command::new(env!(\"", &cargo_bin_exe_loom, "\"))"].concat(),
        ["cargo", "_bin("].concat(),
    ]
}

fn strip_whitespace(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn only_helpers_spawns_the_loom_binary_directly() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = Vec::new();
    rust_files_under(&tests_dir, &mut files);

    let forbidden: Vec<String> = forbidden_patterns()
        .iter()
        .map(|pattern| strip_whitespace(pattern))
        .collect();

    let offenders: Vec<String> = files
        .into_iter()
        .filter(|path| path.file_name().and_then(|n| n.to_str()) != Some("helpers.rs"))
        .filter_map(|path| {
            let content = fs::read_to_string(&path).ok()?;
            let normalized = strip_whitespace(&content);
            forbidden
                .iter()
                .any(|pattern| normalized.contains(pattern.as_str()))
                .then(|| path.display().to_string())
        })
        .collect();

    assert!(
        offenders.is_empty(),
        "these files spawn the loom binary directly instead of going through \
         helpers::loom_cmd(), so a real LOOM_* session variable would leak into \
         the child process unfiltered: {offenders:?}"
    );
}
