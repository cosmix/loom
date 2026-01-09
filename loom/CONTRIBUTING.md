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
```

## Code Standards

- **File size limit:** 400 lines max
- **Function size limit:** 50 lines max
- **No `unwrap()` in production code** - use proper error handling with `anyhow`

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
