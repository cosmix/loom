# WB1 — Rename the command to `update`, rewrite it around the re-exec, delete the zip path

Tier: codex `gpt-5.6-terra`, effort `xhigh`.

## Files you own (write)

- `loom/src/commands/self_update/mod.rs`
- `loom/src/commands/self_update/signature.rs`
- `loom/src/commands/self_update/client.rs`
- `loom/src/commands/self_update/tests.rs`
- `loom/src/commands/self_update/zip.rs` — **delete**
- `loom/src/cli/types.rs` and `loom/src/cli/dispatch.rs` — the rename only
- `loom/src/update_check/mod.rs` and `loom/src/update_check/tests.rs`
- `loom/tests/integration/update_notice.rs`
- `loom/src/commands/init/execute.rs` — ONE string, line 169: `"Run install.sh or loom
  self-update to install loom rules."` becomes `"Run install.sh or loom update to install loom
  rules."`. Nothing else in that file.
- `loom/Cargo.toml` and `loom/Cargo.lock` — only via `cargo remove zip`

The previous stage edited `cli/types.rs` and `cli/dispatch.rs` to add `install-assets`. Re-read
both before editing; do not disturb that work.

Read-only: `loom/src/assets/install.rs`, `loom/src/commands/install_assets.rs` (both landed in the
previous stage and are merged).

## Entry points

In `loom/src/commands/self_update/mod.rs`:

- `execute()` — checks the release, compares versions, calls `update_binary` then
  `update_config_files`.
- `update_binary(release)` — downloads the binary and its `.minisig`, verifies the signature
  **before** anything is written, logs the sha256, then calls `install_binary`.
- `update_config_files(release)` — the whole config-asset download path. This is what goes.
- `download_verify_and_extract_zip`, `save_with_header`, `get_claude_dir`, `checksum_asset` —
  helpers reachable only from `update_config_files`.
- `install_binary(new_binary, current_exe)` in `self_update/install.rs` — leave it alone; it
  already does the atomic swap with rollback.

## What to build

### 0. Rename the command to `update`

One name, no alias. The command stops being self-referential in this plan — it now refreshes
hooks, agents, skills, slash commands and two doctrine files as well as the binary.

- `loom/src/cli/types.rs`: `SelfUpdate` becomes `Update`, and its doc comment (`/// Update loom
  and configuration files`) becomes something that names what it does now, e.g. "Update loom and
  everything it installs". Do **not** add `#[command(alias = "self-update")]`; the old spelling
  disappears.
- `loom/src/cli/dispatch.rs`: `Commands::SelfUpdate => self_update::execute()` becomes
  `Commands::Update => self_update::execute()`.
- `loom/src/update_check/mod.rs`: the notice string
  ``"loom {current} is out of date (latest {latest}) - run `loom self-update` to upgrade."``
  becomes ``run `loom update` ``. The doc comments in that module naming the command follow.
  Its call into `crate::commands::self_update::get_latest_release` is a module path, not a command
  name — leave it. **This file is at exactly 400 lines with no ledger entry:** add no net line; if
  rustfmt rewraps a shortened sentence onto a new line, shorten it further.
- `loom/src/update_check/tests.rs` and `loom/tests/integration/update_notice.rs` both assert the
  notice contains `self-update`. Update both assertions to the new spelling.

**The module keeps its name.** `commands::self_update` accurately describes code that replaces
loom's own binary, it sits beside `update_check` where a second `update` would be easy to misread,
and renaming the directory would churn five `maintainability-baseline.txt` path entries for
nothing. Add one line to the module doc saying it backs the `loom update` subcommand.

After the rename, `rg -q "SelfUpdate" loom/src/cli/types.rs` must find nothing and
`rg -n "self-update" loom/src loom/tests --glob '!loom/src/commands/self_update/**'` must find
nothing (the module keeps its own name, and `client.rs:26`'s `loom-self-update` user agent stays).

### 1. Replace `update_config_files` with a re-exec

After `install_binary` succeeds, run the **new** binary on disk — at the path it was installed
to. Do not place assets from the running process. It still holds the *old* embedded copies; the
freshly-installed binary is the only correct source.

**The path is captured BEFORE the swap. A fresh `env::current_exe()` after the swap is wrong on
Linux.** `install_binary` (`install.rs:35-60`) renames the running binary onto a `NamedTempFile`
backup, renames the new one into place, and unlinks the backup when the `TempPath` drops.
`/proc/self/exe` follows the inode, so after that `env::current_exe()` returns
`<backup> (deleted)` and `Command::new` on it fails with ENOENT — after the binary has already
been replaced. macOS returns the launch path string instead, so the bug is invisible there. The
shape:

```rust
/// Returns the path the new binary now occupies — the same PathBuf that was
/// handed to `install_binary`, captured before the swap. Never call
/// `env::current_exe()` after the swap: on Linux it names a deleted inode.
fn update_binary(release: &Release) -> Result<PathBuf>;

/// Re-executes `exe install-assets`. A spawn failure and a non-zero exit are
/// distinguishable errors, and both name `exe`.
fn run_asset_install(exe: &Path) -> Result<()>;
```

`update_binary` today computes `current_exe` at `mod.rs:231` and returns `()`; return that
`PathBuf` instead. `execute()` has exactly ONE `run_asset_install` call, after the version
comparison, so both branches reach it by construction:

```rust
let exe = if latest_version <= current {
    println!("{} You're running the latest version ({}); refreshing installed assets", …);
    env::current_exe()?          // no swap happened; the running file is still on disk
} else {
    update_binary(&latest)?      // the installed path, captured before the swap
};
run_asset_install(&exe)?;
```

On the update path, before printing success, run `<exe> --version` (a `Command::output`) and
check the output contains the new release's version — the only proof the re-executed file is the
new binary. Failure contract: if `run_asset_install` (or the version probe) fails after a
successful swap, return an error whose message names `exe`, says the binary was updated but the
assets were not, and tells the operator to run `loom install-assets`. The re-exec passes no
flags: the new binary resolves the layout from `loom-install.toml` exactly as a direct
`loom install-assets` does. Keep every function under 50 lines: `run_asset_install`, the version
probe and the branch above are separate small functions.

### 2. Delete the config-asset download path

Remove `update_config_files`, `download_verify_and_extract_zip`, `save_with_header`,
`get_claude_dir` and `checksum_asset`; delete `self_update/zip.rs` and its `mod zip;` declaration;
remove `parse_checksums` and `verify_checksum` from `signature.rs`, keeping
`verify_binary_signature` and `compute_sha256_checksum`. Then run `cargo remove zip` from `loom/`
— never hand-edit the manifest. The `crate::skills::apply_install_layout(&claude_dir)` call at
`mod.rs:328` goes with `update_config_files`; that was its only caller, and another worker in this
stage deletes the function itself — do not touch `skills/`.

Check `client.rs` afterwards: `download_text_with_limit` is still needed for the `.minisig` body,
but any helper left with no caller must go rather than sit dead.

`SHA256SUMS.txt` stops being downloaded. It was only ever used to verify the config assets, which
now travel inside a binary the minisign signature already covers; a sha256 row in an unsigned file
served from the same host was the weaker check. The release keeps publishing it for humans.

Keep every other security property exactly as it is: the signature is still mandatory, still
verified before any byte is written to disk, and the sha256 of the binary is still logged.

### 3. Tests

Delete the tests that covered zip extraction, zip-bomb limits, path traversal in archives and
checksum parsing — the code they guarded is gone. Keep every signature test.

Add tests for the new seam. `run_asset_install` takes a path, so point it at a script in a
`TempDir`. Every test name below contains `run_asset_install` — the stage's acceptance greps the
test list for that token:

- `run_asset_install_invokes_install_assets`: a stub executable that records its argv into a
  file; the test asserts it was invoked with exactly `install-assets`;
- `run_asset_install_reports_nonzero_exit`: a stub that exits non-zero; `Err`, and the message
  names the failure and the path;
- `run_asset_install_reports_missing_binary`: a path that does not exist; `Err`, not a panic.

Never let a test invoke a real `loom install-assets`, and never let one call it without
`--claude-dir` and `--codex-dir` — the bare form writes the operator's real `~/.claude` and
`~/.codex`.

## Done means

- `cargo build --manifest-path loom/Cargo.toml` succeeds.
- `cargo test --manifest-path loom/Cargo.toml --lib commands::self_update::`,
  `--lib update_check::` and `--test integration` all pass.
- `loom/target/debug/loom update --help` exits 0 and `loom/target/debug/loom self-update --help`
  exits non-zero.
- `rg -q "^zip" loom/Cargo.toml` finds nothing; `loom/src/commands/self_update/zip.rs` is gone.

## Constraints the graph will not show you

- `loom/maintainability-baseline.txt` records
  `file src/commands/self_update/mod.rs 465`, `file src/commands/self_update/tests.rs 588`,
  `function src/commands/self_update/mod.rs download_verify_and_extract_zip 117`,
  `function src/commands/self_update/mod.rs update_config_files 86` and
  `function src/commands/self_update/zip.rs safe_extract_path 54`. All five go stale when your
  work lands, and `function src/commands/self_update/mod.rs update_binary 63` changes shape (it
  now returns the path). Stale entries fail the gate exactly as growth does. **You do not own
  that file** — report every number the gate prints to the orchestrator, which reconciles the
  ledger in both directions.
- Do not run `git` at all. Do not run the full test suite, the linter or the formatter.
