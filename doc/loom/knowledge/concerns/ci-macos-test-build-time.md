# CI macOS Test Build Time

> macOS lib test build swaps on the 7 GB runner; nightly split and open options

## The Lib Test Crate's rustc Needs More Memory Than the macOS Runner Has

Building the `loom` lib test binary (`cargo test --lib --no-run`, the library plus about 14,400 in-crate tests in one rustc invocation) peaks at 10.4 GB resident with `debug = 2`, 9.2 GB with `line-tables-only` and 8.5 GB with `debug = 0`; `codegen-units = 16` changes nothing. Measured on a 16-core, 128 GB Mac with `CARGO_INCREMENTAL=0`, each build takes 63-76 s, so debuginfo does not change build time where memory is ample.

GitHub's `macos-latest` arm64 runner has 3 vCPUs and 7 GB RAM; `ubuntu-latest` has 4 vCPUs and 16 GB. In run 44879373671 the same build took 14m 01s on macOS (Flake check (macos)) against 3m 20s on Linux, and the `--all-targets` build took 19m 16s against 3m 07s: the macOS build swaps for most of its length.

How CI responds (`.github/workflows/`):

- `CARGO_PROFILE_DEV_DEBUG: "0"` in `.github/workflows/ci.yml` and `.github/workflows/nightly.yml` lowers the peak by about 2 GB. Panic messages keep file and line; backtraces lose line numbers.
- The macOS test suites (lib, integration, host-job lane) run in `.github/workflows/nightly.yml` (schedule, `workflow_dispatch`, pull requests), so no push waits on them. `.github/workflows/ci.yml` keeps the macOS lint, flake-check and hook jobs; Flake check (macos) still builds the lib test binary and is the longest push job.

Open options: a larger macOS runner (`macos-latest-xlarge`, billed per minute) removes the swap; moving the in-crate tests out of the lib crate, or splitting the crate, would bring a single rustc under 7 GB. Merging the 113 integration-test crates under `loom/tests/` into one would cut about 112 link steps on both runners.
