# C3: token flow model (codex unit)

Read doc/plans/briefs/token-flow/common.md sections 2 and 3 first. Do not run git. Do not touch any path under .loom.

## Files

- Write: loom/src/tokens/model.rs and loom/src/tokens/model_tests.rs.
- Read only: loom/src/quota/cache.rs (reject_symlink, pub(crate)), loom/src/tokens/mod.rs (already declares `pub mod model;` and re-exports `pub use model::*;`).

## Steps

1. Write model.rs from the listing in common.md section 3, with every constant, type, derive and serde attribute exactly as listed.
   - TokenUsage::fresh is the saturating sum input + cache_write + output.
   - TokenUsage::add is field-wise saturating.
   - flow_path returns work_root.join(TOKENS_DIR).join(FLOW_FILE).
   - read_flow returns None, without panicking or logging, when:
     - the path is missing;
     - crate::quota::cache::reject_symlink(&path) errors;
     - `std::fs::symlink_metadata(&path)` is not a regular file (`file_type().is_file()` false: a directory, FIFO or socket), checked BEFORE opening, because opening a FIFO for reading blocks and the collector runs on the 1 s broadcast path;
     - the metadata length is greater than MAX_FLOW_BYTES;
     - reading through `File::open(&path)?.take(MAX_FLOW_BYTES + 1)` yields more than MAX_FLOW_BYTES bytes (the file grew after the stat);
     - the read fails;
     - serde_json fails.
2. Declare the tests with `#[cfg(test)] #[path = "model_tests.rs"] mod tests;`.
3. Write unit tests:
   - a TokenFlow round-trips through serde_json;
   - kind serializes as "main", "subagent" and "codex";
   - provider serializes as "claude" and "codex";
   - a None agent_type serializes as null (the key is present);
   - read_flow on a missing dir returns None;
   - read_flow on a written file returns the flow;
   - read_flow on a symlinked flow.json returns None (std::os::unix::fs::symlink inside a tempfile::TempDir);
   - read_flow on a file of MAX_FLOW_BYTES + 1 bytes returns None;
   - read_flow when flow.json is a directory returns None;
   - fresh saturates at u64::MAX.

Done when your files compile and your tests are written. Your only check is `cargo test --lib --no-run` from loom/: TMPDIR is read-only inside the codex sandbox, so tests that create temp files cannot run there, and the orchestrator runs `cargo test --lib tokens::` after your wave. A compile error in a file you do not own is not yours: report it and stop; never edit that file and never wait for it. Keep both files under 250 lines, functions under 50 lines, rustfmt clean.
