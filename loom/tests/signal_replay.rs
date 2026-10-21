//! Replay harness: renders every stage signal of a frozen plan through format_signal_with_metrics (reached through generate_signal_with_metrics, which builds the production embedded context first) and every worker query through loom hook worker-brief, and prints bytes per section.
//!
//! Invocation, from `loom/`: `cargo test --test signal_replay -- --ignored --nocapture`.
//! With `SIGNAL_REPLAY_OUT=<file>` set, the report is also written to that file.
//! Later plans locate this harness by its path, `tests/signal_replay.rs`.
//! The output format `signal-replay v1` is a contract with those plans; the
//! fixture under `tests/fixtures/signal_replay/` is frozen input.

#[path = "integration/helpers.rs"]
// only loom_cmd() is used; the shared module serves the integration target
#[allow(dead_code)]
mod helpers;
#[path = "signal_replay/input.rs"]
mod input;
#[path = "signal_replay/render.rs"]
mod render;
#[path = "signal_replay/report.rs"]
mod report;

use std::fs;

use tempfile::TempDir;

/// The report of a fresh run; `nested` roots it one directory deeper so a
/// leaked temp path changes the output.
fn run(nested: bool) -> (TempDir, report::Report) {
    let base = TempDir::new().expect("create replay root");
    let root = if nested {
        base.path().join("n")
    } else {
        base.path().to_path_buf()
    };
    fs::create_dir_all(&root).expect("create replay root");
    let report = report::render(&root).unwrap_or_else(|e| panic!("replay failed: {e:#}"));
    (base, report)
}

#[test]
#[ignore = "replay harness: cargo test --test signal_replay -- --ignored --nocapture"]
fn signal_replay() {
    let started = std::time::Instant::now();
    let (_keep, first) = run(false);
    let (_keep_nested, second) = run(true);

    assert_eq!(
        first.lines, second.lines,
        "the report differs between two roots: a path or a clock leaked into it"
    );
    report::check_sections(&first.lines).unwrap_or_else(|e| panic!("{e:#}"));
    report::check_shape(&first.lines, 6).unwrap_or_else(|e| panic!("{e:#}"));
    assert!(
        first.delivering_records > 0,
        "no worker delivery record delivered anything; queries and envelopes: {:?}",
        first.envelopes
    );

    let text = first.lines.join("\n");
    println!("{text}");
    if let Ok(path) = std::env::var("SIGNAL_REPLAY_OUT") {
        fs::write(&path, format!("{text}\n")).expect("write SIGNAL_REPLAY_OUT");
        let written = fs::read_to_string(&path).expect("read SIGNAL_REPLAY_OUT back");
        assert_eq!(written.lines().collect::<Vec<_>>(), first.lines);
    }
    eprintln!("signal replay took {:?}", started.elapsed());
}
