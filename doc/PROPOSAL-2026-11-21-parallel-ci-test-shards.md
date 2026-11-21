# Proposal: Build Once, Run the Test Suite in Parallel Shards

## Problem

CI wall-clock time is set by two costs: compiling the `loom` test targets (about 3 min on `ubuntu-latest`, 14–19 min on `macos-latest`, where the lib test crate's rustc needs about 10 GB on a 7 GB runner and swaps) and running the lib tests (395 s on Linux, 857 s on macOS in run 44879373671). Splitting the suite into more jobs that each compile repeats the compile cost per job, so more runners alone do not help.

## Constraints

- loom is a public repository: standard GitHub-hosted runners are free with unlimited minutes. A personal Pro account allows about 40 concurrent jobs, at most 5 of them macOS (GitHub's documented limits, not verified on this account).
- Larger runners need an organization on Team or Enterprise; `cosmix` is a user account. The Intel macOS image has 14 GB but tests x86_64 and is being retired.
- Suite size: about 14,360 lib tests and 1,270 integration tests.
- 823 tests are `#[serial]` (`serial_test`); they run one at a time and set a floor on wall-clock time. They serialize mostly because of shared process state: 81 files call `std::env::set_var` (`HOME`, `PATH`, `LOOM_*`); others change the working directory, take `flock` probes, or use shared tmux servers, sockets, sandbox launchers and files under `~/.loom`.
- `.cargo/config.toml` pins `RUST_TEST_THREADS = 4`.

## Proposal

1. A build job per OS runs `cargo nextest archive` and uploads the archive as an artifact. This is the only compile.
2. N shard jobs per OS download the archive and run `cargo nextest run --archive-file <archive> --partition count:k/N`, compiling nothing.
3. Map the serialization `#[serial]` provides onto nextest:
   - nextest runs each test in its own process, so `#[serial]` (an in-process lock) no longer serializes;
   - tests serial only because they mutate the environment or working directory need nothing: process isolation removes the race (the memory-journals pass-cache failure was this race);
   - tests sharing an external resource (tmux, ports, `~/.loom` files, lock files, sandbox launchers) go into nextest test groups with `max-threads = 1`, which needs an audit of the 823 serial tests.

Rough estimate: Linux push tests from about 10 min to 5–6 min with 4 shards; macOS lib tests from 14 min to about 4 min, with the single build still swapping.

## First Step

One nextest run on each OS without sharding, keeping JUnit output, to get per-test timings: whether the serial tests or a few slow tests set the wall-clock time. Decide the shard count and the audit scope from that.

## Costs and Risks

- The serial-test audit is the bulk of the work (estimated a day or more).
- A test misclassified as environment-only that actually shares an external resource becomes a flake under nextest.
- Archive upload and download adds about a minute per shard; the archive counts toward artifact storage until it expires, so set a short `retention-days`.
