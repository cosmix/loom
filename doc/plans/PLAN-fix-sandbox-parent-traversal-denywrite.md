# Fix: sandbox.filesystem.denyWrite parent-traversal paths block shell-snapshots and loom CLI

## Context

Two errors are occurring in loom-managed worktree sessions (and even the main project session when `settings.local.json` was written by loom):

1. **Shell snapshots blocked:**

   ```
   zsh:source:1: operation not permitted: /Users/dkaponis/.claude/shell-snapshots/snapshot-zsh-*.sh
   ```

2. **`loom` CLI can't get CWD:**

   ```
   Error: Failed to get current directory
   Caused by: Operation not permitted (os error 1)
   ```

**Root cause:** `sandbox.filesystem.denyWrite` includes `../../**`. The macOS sandbox-exec resolves this relative to the project root. For a project at `/Users/dkaponis/src/loom`, `../../**` resolves to `/Users/dkaponis/**` — blocking ALL writes (and possibly reads/execute) across the entire home directory.

This is the **exact same class of bug** we already fixed for `denyRead` (see comment at `settings.rs:130-135`). We removed `denyRead` from `sandbox.filesystem` entirely because the OS sandbox was too aggressive. But we left `denyWrite` paths in `sandbox.filesystem.denyWrite`, and the parent-traversal pattern `../../**` causes the same problem.

## Fix

**File:** `loom/src/sandbox/settings.rs` — `generate_settings_json()` (lines 136-146)

Filter parent-traversal paths (`../`) out of `sandbox.filesystem.denyWrite`. These paths will still be enforced via `permissions.deny Write()` entries (tool-level restriction, lines 166-169), which don't affect the OS sandbox.

Project-relative paths like `doc/loom/knowledge/**` are safe to keep in `sandbox.filesystem.denyWrite`.

### Before (lines 136-146)

```rust
let mut fs_sandbox = json!({});
if !config.filesystem.deny_write.is_empty() {
    fs_sandbox["denyWrite"] = json!(config.filesystem.deny_write);
}
```

### After

```rust
let mut fs_sandbox = json!({});
// Filter parent-traversal paths from OS-level denyWrite.
// Paths like "../../**" resolve relative to the project root in macOS
// sandbox-exec, causing overly broad restrictions (e.g. blocking the
// entire home directory). These are still enforced via permissions.deny
// Write() entries at the tool level.
if !config.filesystem.deny_write.is_empty() {
    let safe_deny_write: Vec<&str> = config
        .filesystem
        .deny_write
        .iter()
        .filter(|p| !p.contains("../"))
        .map(|s| s.as_str())
        .collect();
    if !safe_deny_write.is_empty() {
        fs_sandbox["denyWrite"] = json!(safe_deny_write);
    }
}
```

### Tests to update

1. **`test_generate_settings_with_filesystem`** (`settings.rs:325`): Update assertion — `sandbox.filesystem.denyWrite` should no longer contain `../../**` (only `doc/loom/knowledge/**`-style paths that don't have `../`). Currently this test uses `deny_write: [".work/**"]` which has no `../`, so it should still pass.

2. **Add new test `test_deny_write_parent_traversal_not_in_os_sandbox`**: Verify that `../../**` in `deny_write` does NOT appear in `sandbox.filesystem.denyWrite` but DOES appear in `permissions.deny`.

3. Existing test `test_deny_read_not_in_os_sandbox` (line 566) is the model for the new test.

## Verification

```bash
cd loom
cargo test sandbox -- --nocapture   # Run sandbox-related tests
cargo clippy -- -D warnings         # No warnings
```

After building and installing, run a loom session and verify:

- No `zsh:source: operation not permitted` errors on shell-snapshots
- `loom memory` commands work without "Failed to get current directory"
