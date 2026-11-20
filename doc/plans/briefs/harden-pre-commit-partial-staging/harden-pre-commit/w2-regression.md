# W2: Black-Box Regression and CI Wiring

## Objective

Create a deterministic shell regression that executes `loom/.githooks/pre-commit` in a temporary
Git repository, then wire that regression into the existing CI hook job.

## Files

- Own: `scripts/test-pre-commit-partial-staging.sh`
- Own: `.github/workflows/ci.yml`
- Read-only contract: `loom/.githooks/pre-commit`
- Do not edit any other file.

## Required Test Harness

1. Use `${TMPDIR:-/tmp}` and create a fresh temporary directory with cleanup on exit.
2. Initialize a real nested Git repository containing a `loom/` directory and configure a local
   test identity. Use real Git index operations.
3. Put fake `cargo` and `bunx` executables first on `PATH`. Each fake records invocation and exits
   successfully so the fixture tests hook control flow without compiling or accessing a network.
4. Invoke the checked-in hook by absolute path. Do not copy its logic into the test.

## Required Cases

- **Partial path:** commit a base Rust file, stage one change, then add another working-tree-only
  change. The hook must exit nonzero, name the path, invoke neither fake tool, and leave both
  `git show :path` and the working-tree bytes exactly equal to their values before the hook.
- **Fully staged path:** stage the complete Rust change. The hook must exit zero and reach the fake
  Cargo path, proving the guard did not disable the normal flow.
- **Filename with spaces:** exercise the partial case with a staged path containing spaces so a
  newline/word-splitting implementation fails.
- **Unstaged-only control:** an unrelated unstaged path must not be reported as a staged conflict.

Give each failed assertion a precise message and make the script print a compact success line.
Avoid output assertions that depend on color or command ordering beyond the contract above.

## CI Wiring

In `.github/workflows/ci.yml`, add a step to the existing `hook-syntax` job immediately after the
shell syntax step. Run `./scripts/test-pre-commit-partial-staging.sh`; do not create a separate job
or install dependencies.

## Constraints

- The test must fail if the new guard is removed or moved below `cargo fmt`.
- Use POSIX utilities plus Bash and Git available on both Ubuntu and macOS developer systems.
- Keep all writes inside the temporary repository.
- Use `apply_patch`. Never run Git as implementation workflow and never spawn subagents.
- W1 may still be editing the hook; do not run the behavioral fixture in this worker.

## Narrow Check

Run exactly once after editing:

```bash
bash -n scripts/test-pre-commit-partial-staging.sh
```

The orchestrator runs the complete fixture after both workers return.
