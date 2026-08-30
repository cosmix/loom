# Detached Spawn In Tests

> Never spawn a process from a test that can outlive the test process.

## Never Spawn a Surviving Process From a Test (2026-08-21)

**What happened:** the prompt hook's background self-heal
(`commands/hook/reconcile_graph.rs::spawn_if_needed`, Appendix A.12 of the
retrieval-precision proposal) launches a detached `loom hook reconcile-graph`
with `process_group(0)` so it survives the hook's exit. Seven tests in
`commands/hook/tests_user_prompt_e2e.rs` drive `retrieve_for_prompt`, which
calls it. One of those tests removes `LOOM_WORK_DIR` on purpose to exercise the
checkout fallback — so `WorkDir::new(".")` searched UPWARD from the crate
directory and resolved to the real loom checkout instead of a fixture. Its
semantic layer is genuinely stale, so the spawn fired against an 8,500-node
repository. Every `cargo test` run leaked more children, each walking the whole
tree through tree-sitter. The machine exhausted 125 GB and had to be rebooted.

**Why:** three ordinary decisions composed into a runaway.

- `WorkDir::new` searching upward is correct for reading and dangerous for
  writing: a test's working directory is inside the real repo, so an unset
  environment variable does not fail, it silently retargets the real checkout.
- `process_group(0)` is exactly right for the production hook and exactly wrong
  under a test harness, which exits without reaping what it started.
- `cargo test` reports nothing about a leaked child. The suite goes green while
  the children keep running, so the cost lands on the next run, not this one.

**Prevention:**

- Guard process creation at the lowest level — the function that calls
  `Command::spawn` — never at the caller. The staleness check, lock decision and
  lock claim above it stay under test; only the spawn is suppressed.
- `cfg!(test)` covers `#[cfg(test)]` modules in this crate but NOT the
  integration targets under `loom/tests/*.rs`, which link the lib built without
  it. Cover both, e.g. an `AtomicBool` defaulting to `!cfg!(test)`.
- Never start expensive background work against a project root reached only by
  an upward directory walk. Inference is fine for READING an index, not for
  spawning work against it: require an explicit `LOOM_WORK_DIR`, or an existing
  `.loom/cache/context-v1` proving loom already maintains this checkout.
- When reviewing any `Command::spawn`, ask what happens when a test reaches it.

**Fix:** spawn guarded by an `AtomicBool` defaulting to `!cfg!(test)`, plus an
inferred-root refusal in `spawn_if_needed`, plus a test asserting the guard is
in force so it cannot regress silently.
