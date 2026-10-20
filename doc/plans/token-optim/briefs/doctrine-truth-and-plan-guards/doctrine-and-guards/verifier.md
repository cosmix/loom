# Verifier round: stage `doctrine-and-guards`

Run every step below on the finished worktree, read stderr for each, and report pass or fail per step with the exit code and, for a failure, the first failing test or error lines. Write nothing outside `loom/target/` (an ignored directory; steps 7, 8 and 10 keep their caches there).

Before step 7, from the repository root: `mkdir -p loom/target/gate-cache/bun loom/target/gate-cache/tmp`.

From `loom/` (every step of the repository's pre-push gate, `loom/.githooks/pre-push`, in its order, plus the doctest run that `--all-targets` leaves out):

1. `cargo build`
2. `cargo fmt --check`
3. `cargo clippy --all-targets -- -D warnings`
4. `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
5. `env -u GIT_INDEX_FILE -u GIT_DIR -u GIT_WORK_TREE cargo test --all-targets --no-fail-fast`
6. `cargo test --doc --workspace --all-features -- --test-threads=1`
7. `cargo audit --db "$PWD/target/gate-cache/advisory-db"` (the pre-push gate runs `cargo audit`; `--db` keeps the advisory database out of `~/.cargo`, and the stage's sandbox grants the network it fetches over)

From the repository root (the markdown and flake steps of the pre-push gate, then CI's four hook steps from `.github/workflows/ci.yml`):

1. `git ls-files '*.md' | grep -v '^doc/plans/' | grep -v '^loom/tests/fixtures/' | BUN_INSTALL_CACHE_DIR="$PWD/loom/target/gate-cache/bun" TMPDIR="$PWD/loom/target/gate-cache/tmp" xargs bunx markdownlint-cli2` — no `--fix`. Pass only when it exits 0 AND its output contains `markdownlint-cli2 v`: without that banner the linter never ran (bun could not reach its cache or the registry), which is a failure.
2. `./scripts/flake-check.sh`
3. `./scripts/check-hook-syntax.sh`
4. `bash loom-hooks/tests/run-all.sh`
5. `LOOM_HOOK_TEST_BSD=1 bash loom-hooks/tests/run-all.sh`
6. `TMPDIR="$PWD/loom/target/gate-cache/tmp" ./scripts/test-pre-commit-partial-staging.sh`

Then every acceptance criterion and every `after_stage` command the stage signal lists, from `loom/`, in order.

Baseline at `6fee4ecd` on the planning host (outside a sandbox): steps 1-6 and 10-12 exit 0; step 5 ran 8,576 tests with 0 failed and 7 ignored in about 2.5 minutes; step 6 ran 6 doctests; steps 11 and 12 each passed 102 hook tests (this stage adds one, so 103). Steps 7, 8, 9 and 13 were not run while planning. A failure in a test or file this stage did not touch is still a failure to report: name it, and say whether it fails at the stage's base too.
