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

### Working in a loom worktree

- **Do not run `cargo fmt` while sibling agents are working.** It ignores path arguments and
  formats the whole crate, clobbering files another agent owns. Use
  `rustfmt --edition 2021 <file>` on your own files.
- `cargo test` accepts exactly **one** testname filter; extra filters make it run zero tests.
- Derived context state under `.loom/cache/` and paths reached through the `.work` symlink
  resolve to the **main** project root, shared across worktrees. Treat them as shared state.

## Release Process

### Binary Signing with Minisign

All release binaries are cryptographically signed using [minisign](https://jedisct1.github.io/minisign/).

#### Public Key

```text
RWTxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

> **Note:** Replace with actual public key after keypair generation.

#### Verifying a Release

Users can verify downloaded binaries:

```bash
# Install minisign
# macOS: brew install minisign
# Linux: apt install minisign

# Download binary and signature
curl -LO https://github.com/cosmix/loom/releases/download/vX.Y.Z/loom-x86_64-unknown-linux-gnu
curl -LO https://github.com/cosmix/loom/releases/download/vX.Y.Z/loom-x86_64-unknown-linux-gnu.minisig

# Verify signature
minisign -Vm loom-x86_64-unknown-linux-gnu -P 'RWTxxxxxx...'
```

#### Release Signing (Maintainers Only)

1. **One-time setup** - Generate keypair (store private key securely):

   ```bash
   minisign -G -p loom.pub -s loom.key
   ```

2. **Sign release binaries**:

   ```bash
   minisign -Sm loom-x86_64-unknown-linux-gnu -s loom.key
   minisign -Sm loom-x86_64-apple-darwin -s loom.key
   minisign -Sm loom-x86_64-pc-windows-msvc.exe -s loom.key
   ```

3. **Upload both binary and `.minisig` file** to the GitHub release.

4. **Update public key** in:
   - `src/commands/self_update.rs:18` (`MINISIGN_PUBLIC_KEY` constant)
   - This file (CONTRIBUTING.md)

### CI/CD Integration

For automated releases, store the private key as a GitHub secret and add to your workflow:

```yaml
- name: Sign release binaries
  env:
    MINISIGN_KEY: ${{ secrets.MINISIGN_PRIVATE_KEY }}
  run: |
    echo "$MINISIGN_KEY" > loom.key
    for binary in loom-*; do
      minisign -Sm "$binary" -s loom.key
    done
    rm loom.key
```

## Security

- Report security vulnerabilities privately via GitHub Security Advisories
- See `doc/plans/PLAN-0002-loom-security-remediation.md` for security audit details
