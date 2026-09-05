# Contributing to loom

## Development Setup

```bash
cd loom
cargo build
cargo test
```

## Quality Gates

All changes must pass these checks before merge:

```bash
cargo check                                    # Compilation
cargo test                                     # All tests pass
cargo clippy --all-targets -- -D warnings      # No lint warnings
cargo audit                                    # No security vulnerabilities
cargo test --test maintainability              # Size limits and the debt ledger
cargo build --no-default-features              # Builds without the tree-sitter grammars
```

> **Who runs these:** you, or the main agent of a loom session. When loom is driving, these are
> exactly the commands `hooks/subagent-verify-guard.sh` blocks for **subagents** — a subagent may
> run at most one narrowly-scoped check on the files it changed, and the main agent owns
> whole-project verification. `integration-verify` stages are carved out and run the full suite.
> See "Verification Is the Main Agent's Job" in the README.

## Code Standards

- **File size limit:** 400 lines max
- **Function size limit:** 50 lines max
- **No `unwrap()` in production code** - use proper error handling with `anyhow`

### The maintainability ledger

`maintainability-baseline.txt` records the files and functions that already exceed those
limits, and `cargo test --test maintainability` enforces it as an **exact match, not a
ceiling** — it fails on shrinkage just as loudly as on growth. Practical consequences:

- A file recorded at its exact line count cannot take even one more line, so check the ledger
  *before* you start: `rg <your-file> maintainability-baseline.txt`.
- If you legitimately shrink a ledgered entry, **lower its recorded number** in the same change.
- The two size limits can fight: extracting a helper to get a function under 50 lines can push
  the file over 400. Prefer extracting into an unledgered sibling module.
- When splitting a file, use the edition-2021 layout `<name>.rs` plus a `<name>/` directory.
  Avoid `<name>/mod.rs` — it changes the path, and loom plans pin verification patterns to
  exact file paths.

The scanner parses every `.rs` file under the crate, `tests/fixtures/` included, and errors on
unbalanced braces. A deliberately-unparseable fixture must therefore not carry a real `.rs`
extension — name it `<name>.rs.broken`.

### Optional grammars

Tree-sitter source extraction sits behind the default-on `source-graph` feature, with every
grammar exact-pinned (`=x.y.z`). Keep `cargo build --no-default-features` green: without the
feature, extraction must degrade to file-level lexical nodes rather than fail to build, so a
host with no C toolchain can still build loom. If you change a grammar pin, the embedded query,
or the shape of the tree-sitter walk, bump `ExtractorIdentity` — otherwise cached extractions
from the previous build are silently reused.

### Web dashboard

`web/` is a Bun + Vite React project backing `loom status --web`. `web/dist` is committed and
embedded into the loom binary at compile time by `build.rs`, so the shipped binary needs no
Node toolchain. Rebuild and commit `dist/` together with any change under `web/src/`:

```bash
cd web && bun install && bun run build
```

### Working in a loom worktree

- **Do not run `cargo fmt` while sibling agents are working.** It ignores path arguments and
  formats the whole crate, clobbering files another agent owns. Use
  `rustfmt --edition 2021 <file>` on your own files.
- `cargo test` accepts exactly **one** testname filter; extra filters make it run zero tests.
- Derived context state under `.loom/cache/` and paths reached through the `.loom/work` symlink
  (or the legacy `.work` symlink, for a workspace that already resolved to it) resolve to the
  **main** project root, shared across worktrees. Treat them as shared state.

## Release Process

Releases are cut by tag. `.github/workflows/release.yml` does the rest - building,
signing, verifying and publishing. Nothing is signed or uploaded by hand.

### Cutting a release

```bash
git tag -a vX.Y.Z -m "loom X.Y.Z"
git push origin vX.Y.Z
```

The version is derived from the tag by `build.rs` (`derive_version` in
`src/version/derive.rs`) and embedded as `LOOM_VERSION`. The `version` field in
`Cargo.toml` is not the source and needs no bump. A `verify-version` job compares
`loom -v` against the tag and fails the release on a mismatch.

To exercise the pipeline without publishing, run the workflow manually with
`dry_run: true` (Actions -> Release -> Run workflow, or
`gh workflow run release.yml -f dry_run=true`). Building, signing and signature
verification all run; release creation is additionally gated on a tag ref, so a
manual dispatch cannot publish.

### Published assets

| Asset | Target |
| ------------------- | -------------------------- |
| `loom-linux-x86_64` | `x86_64-unknown-linux-gnu` |
| `loom-darwin-arm64` | `aarch64-apple-darwin`     |

Each ships with a `.minisig` signature, alongside a `SHA256SUMS.txt` covering both.

Three places name these assets and change together: the workflow's build matrix and
release `files:` list, the platform cases in `install.sh`, and `RELEASE_ASSETS` in
`src/commands/self_update/mod.rs`. A platform absent from `RELEASE_ASSETS` is still
detected by `get_target()`, so `loom update` reports the triple it could not serve
rather than failing anonymously.

### Signing

Binaries are signed with [minisign](https://jedisct1.github.io/minisign/) using the
`MINISIGN_PRIVATE_KEY` repository secret (Settings -> Secrets and variables ->
Actions).

The secret must hold a **password-less** secret key - the whole key file, comment
line included. The workflow pipes it straight into `minisign -Sm` with nothing on
stdin, so a password-protected key stalls the job. Generate one with `-W`:

```bash
minisign -G -W -p loom.pub -s loom.key
```

The matching public key lives in `src/commands/self_update/signature.rs` as
`MINISIGN_PUBLIC_KEY`, and that constant is the single source of truth. After
signing, the workflow reads the key out of that file, verifies every binary against
it, and quotes the same value in the release notes. A secret from a different
keypair fails the run before anything is published.

### Rotating the signing key

The public key is compiled into every binary, so a released binary only accepts
updates signed by the key it shipped with. Rotating breaks `loom update` for
everyone already installed - they have to reinstall. Rotate only when the private
key is compromised or lost, and change the secret and `MINISIGN_PUBLIC_KEY`
together.

### Verifying a release

```bash
# macOS: brew install minisign
# Linux: apt install minisign

curl -LO https://github.com/cosmix/loom/releases/download/vX.Y.Z/loom-linux-x86_64
curl -LO https://github.com/cosmix/loom/releases/download/vX.Y.Z/loom-linux-x86_64.minisig

minisign -Vm loom-linux-x86_64 -P <public key from the release notes>
```

`loom update` performs the same check automatically, against the key embedded in
the running binary.

## Security

- Report security vulnerabilities privately via GitHub Security Advisories
- Known security-relevant limitations are tracked in `doc/loom/knowledge/concerns.md`
- The `sandbox:` block bounds the agent session. `command_confinement` scrubs the
  environment of the commands loom runs from a plan and is not an isolation
  boundary - see "Sandbox Configuration" in the README
