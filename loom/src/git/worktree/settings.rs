//! Worktree settings management
//!
//! Handles creation of settings files (.claude/, CLAUDE.md) for worktrees.
//! Also supports hooks configuration when session context is available.

use anyhow::{Context, Result};
#[allow(unused_imports)] // Required for lock_shared() method on File
use fs2::FileExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use crate::fs::memory::SPOOL_RELPATH;
use crate::hooks::{setup_hooks_for_worktree, HooksConfig};
use crate::plan::schema::PermissionMode;

/// Whether a repo-relative path is worktree scaffolding loom itself creates.
///
/// `create_worktree` plants `.work` (symlink), `.claude/` (dir with a CLAUDE.md
/// symlink and generated settings) and — when the checkout has none — a root
/// `CLAUDE.md` symlink. Repos that gitignore these see nothing; repos that do
/// not see them as untracked. Callers reading `git status` to judge whether a
/// worktree holds *agent work* must discount them either way.
///
/// Also discounted: `.loom/memory-spool.jsonl` and `.loom/cache/`, loom's own
/// runtime paths (see `crate::fs::memory::spool` and `crate::context::store`).
/// Unlike the paths above these are written lazily during a stage's
/// execution, not planted by `create_worktree` — but they are just as much
/// loom's own output as the rest, so they discount the same way. Note this is
/// narrower than `.loom/` as a whole: a project may legitimately track
/// `.loom/config.toml`, which must NOT be discounted here.
///
/// Keep this in sync with the scaffold `create_worktree` writes.
pub fn is_worktree_scaffold_path(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    path == ".work"
        || path == "CLAUDE.md"
        || path == ".claude"
        || path.starts_with(".claude/")
        || path.starts_with(".work/")
        || path == SPOOL_RELPATH
        || path == ".loom/cache"
        || path.starts_with(".loom/cache/")
}

/// Creates or restores the .work symlink in a worktree.
///
/// Used during worktree creation and merge failure recovery.
/// The symlink points from .worktrees/{stage_id}/.work to ../../.work (the main repo's .work/).
pub fn ensure_work_symlink(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_work_dir = repo_root.join(".work");
    let worktree_work_link = worktree_path.join(".work");
    let relative_work_path = Path::new("../../.work");

    if main_work_dir.exists() && !worktree_work_link.exists() {
        #[cfg(unix)]
        std::os::unix::fs::symlink(relative_work_path, &worktree_work_link)
            .with_context(|| "Failed to create .work symlink in worktree")?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(relative_work_path, &worktree_work_link)
            .with_context(|| "Failed to create .work symlink in worktree")?;
    }
    Ok(())
}

/// Set up .claude/ directory for worktree
///
/// We create a real directory and symlink CLAUDE.md from main repo.
/// settings.json is created separately by the hooks system with merged global + session hooks.
/// This ensures:
/// 1. Instructions (CLAUDE.md) are shared
/// 2. Permissions (settings.json) include both global hooks and session-specific hooks
pub fn setup_claude_directory(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_claude_dir = repo_root.join(".claude");
    let worktree_claude_dir = worktree_path.join(".claude");

    if main_claude_dir.exists() && !worktree_claude_dir.exists() {
        // Create real .claude/ directory in worktree
        std::fs::create_dir_all(&worktree_claude_dir)
            .with_context(|| "Failed to create .claude directory in worktree")?;

        // Symlink CLAUDE.md from main repo for instruction inheritance
        let main_claude_md = main_claude_dir.join("CLAUDE.md");
        if main_claude_md.exists() {
            let worktree_claude_md = worktree_claude_dir.join("CLAUDE.md");
            let relative_claude_md = Path::new("../../../.claude/CLAUDE.md");

            #[cfg(unix)]
            std::os::unix::fs::symlink(relative_claude_md, &worktree_claude_md)
                .with_context(|| "Failed to create CLAUDE.md symlink in worktree")?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_file(relative_claude_md, &worktree_claude_md)
                .with_context(|| "Failed to create CLAUDE.md symlink in worktree")?;
        }

        // Create settings.json with trust and auto-accept settings merged with main repo settings
        let main_settings = main_claude_dir.join("settings.json");
        let worktree_settings = worktree_claude_dir.join("settings.json");
        create_worktree_settings(&main_settings, &worktree_settings, worktree_path)?;

        // Copy settings.local.json if it exists (contains user-granted runtime permissions)
        // Use file locking to prevent reading a partially written file during concurrent syncs
        let main_settings_local = main_claude_dir.join("settings.local.json");
        let worktree_settings_local = worktree_claude_dir.join("settings.local.json");
        if main_settings_local.exists() {
            copy_file_with_shared_lock(&main_settings_local, &worktree_settings_local)
                .with_context(|| "Failed to copy settings.local.json to worktree")?;
            // The main repo's copy may carry per-session identity env vars from a
            // previous main-repo session (older loom versions persisted them);
            // they must not leak into this worktree's settings.
            scrub_copied_settings_env(&worktree_settings_local);
        }
    }

    Ok(())
}

/// Symlink project-root CLAUDE.md (distinct from .claude/CLAUDE.md)
///
/// This ensures instances in worktrees have access to project instructions
/// without needing to read from the main repo outside the worktree
pub fn setup_root_claude_md(worktree_path: &Path, repo_root: &Path) -> Result<()> {
    let main_root_claude_md = repo_root.join("CLAUDE.md");
    let worktree_root_claude_md = worktree_path.join("CLAUDE.md");

    if main_root_claude_md.exists() && !worktree_root_claude_md.exists() {
        // Relative path from .worktrees/{stage_id}/CLAUDE.md to ../../CLAUDE.md
        let relative_root_claude_md = Path::new("../../CLAUDE.md");

        #[cfg(unix)]
        std::os::unix::fs::symlink(relative_root_claude_md, &worktree_root_claude_md)
            .with_context(|| "Failed to create root CLAUDE.md symlink in worktree")?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_file(relative_root_claude_md, &worktree_root_claude_md)
            .with_context(|| "Failed to create root CLAUDE.md symlink in worktree")?;
    }

    Ok(())
}

/// Best-effort removal of per-session identity env vars and a stale
/// `LOOM_WORK_DIR` pin from a copied settings file. Leaves the file
/// untouched if it cannot be parsed.
fn scrub_copied_settings_env(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(mut settings) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    let identity_removed = crate::fs::permissions::scrub_session_identity_env(&mut settings);
    let stale_work_dir_removed = crate::fs::permissions::scrub_stale_work_dir_env(&mut settings);
    if identity_removed || stale_work_dir_removed {
        if let Ok(updated) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(path, updated);
        }
    }
}

/// Copy a file with a shared (read) lock on the source.
///
/// This prevents reading a partially written file during concurrent writes.
/// The source file is locked with a shared lock (allowing other readers),
/// and the content is read and written to the destination atomically.
fn copy_file_with_shared_lock(src: &Path, dst: &Path) -> Result<()> {
    // Open the source file and acquire a shared lock
    let src_file = File::open(src)
        .with_context(|| format!("Failed to open source file: {}", src.display()))?;

    src_file
        .lock_shared()
        .with_context(|| format!("Failed to acquire shared lock on: {}", src.display()))?;

    // Read content while holding the lock
    let mut content = Vec::new();
    let mut reader = &src_file;
    reader
        .read_to_end(&mut content)
        .with_context(|| format!("Failed to read source file: {}", src.display()))?;

    // Lock is released when src_file is dropped, but we can write to dst now
    // since we have the complete content

    // Write to destination
    let mut dst_file = File::create(dst)
        .with_context(|| format!("Failed to create destination file: {}", dst.display()))?;

    dst_file
        .write_all(&content)
        .with_context(|| format!("Failed to write to destination file: {}", dst.display()))?;

    Ok(())
}

/// Merge permissions from main repo's settings.local.json into a worktree.
///
/// This is the public interface for refreshing permissions in a worktree.
/// Instead of overwriting, it merges permissions from both sources:
/// - Permissions from the main repo's settings.local.json
/// - Existing permissions in the worktree's settings.local.json (if any)
///
/// This ensures worktree-specific permissions are preserved while still
/// receiving updates from the main repo.
pub fn refresh_worktree_settings_local(worktree_path: &Path, repo_root: &Path) -> Result<bool> {
    let main_settings_local = repo_root.join(".claude/settings.local.json");
    let worktree_settings_local = worktree_path.join(".claude/settings.local.json");

    if !main_settings_local.exists() {
        return Ok(false);
    }

    // Ensure .claude directory exists in worktree
    let worktree_claude_dir = worktree_path.join(".claude");
    if !worktree_claude_dir.exists() {
        std::fs::create_dir_all(&worktree_claude_dir)
            .with_context(|| "Failed to create .claude directory in worktree")?;
    }

    // Read main repo settings with shared lock
    let main_settings = read_settings_with_shared_lock(&main_settings_local)?;

    // Read existing worktree settings (if any)
    let worktree_settings = if worktree_settings_local.exists() {
        read_settings(&worktree_settings_local)?
    } else {
        json!({})
    };

    // Extract permissions from both
    let (main_allow, main_deny) = extract_permissions(&main_settings);
    let (wt_allow, wt_deny) = extract_permissions(&worktree_settings);

    // Merge permissions (union with deduplication)
    let merged_allow = merge_permission_vecs(main_allow, wt_allow);
    let merged_deny = merge_permission_vecs(main_deny, wt_deny);

    // Build merged settings. The base MUST be the worktree's own settings when
    // they exist: they carry session-specific hooks and the stage-resolved
    // permission mode. Using the main repo's settings as base (as this
    // function once did) clobbered all of that mid-session with whatever the
    // last main-repo session left behind. Only permissions are refreshed from
    // the main repo. When the worktree has no settings yet, fall back to the
    // main copy, scrubbed of per-session identity env vars.
    let mut merged = if worktree_settings_local.exists() {
        worktree_settings
    } else {
        let mut base = main_settings.clone();
        crate::fs::permissions::scrub_session_identity_env(&mut base);
        crate::fs::permissions::scrub_stale_work_dir_env(&mut base);
        base
    };
    set_permissions(&mut merged, merged_allow, merged_deny)?;

    // Write merged result
    let content =
        serde_json::to_string_pretty(&merged).with_context(|| "Failed to serialize settings")?;
    std::fs::write(&worktree_settings_local, content)
        .with_context(|| format!("Failed to write {}", worktree_settings_local.display()))?;

    Ok(true)
}

/// Read and parse a settings.json file with a shared lock
fn read_settings_with_shared_lock(path: &Path) -> Result<Value> {
    let file = File::open(path).with_context(|| format!("Failed to open {}", path.display()))?;

    file.lock_shared()
        .with_context(|| format!("Failed to acquire shared lock on {}", path.display()))?;

    let mut content = String::new();
    let mut reader = &file;
    reader
        .read_to_string(&mut content)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    // Lock released when file is dropped
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", path.display()))
}

/// Read and parse a settings.json file
fn read_settings(path: &Path) -> Result<Value> {
    if !path.exists() {
        return Ok(json!({}));
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", path.display()))
}

/// Extract allow and deny permission arrays from settings
fn extract_permissions(settings: &Value) -> (Vec<String>, Vec<String>) {
    let permissions = settings.get("permissions");

    let allow = permissions
        .and_then(|p| p.get("allow"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let deny = permissions
        .and_then(|p| p.get("deny"))
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    (allow, deny)
}

/// Merge two permission vectors, removing duplicates
fn merge_permission_vecs(a: Vec<String>, b: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result = Vec::new();

    for perm in a.into_iter().chain(b) {
        if seen.insert(perm.clone()) {
            result.push(perm);
        }
    }

    result
}

/// Set permissions in a settings Value
fn set_permissions(settings: &mut Value, allow: Vec<String>, deny: Vec<String>) -> Result<()> {
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Settings must be a JSON object"))?;

    let permissions = obj
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions must be a JSON object"))?;

    if !allow.is_empty() {
        permissions.insert("allow".to_string(), json!(allow));
    }
    if !deny.is_empty() {
        permissions.insert("deny".to_string(), json!(deny));
    }

    Ok(())
}

/// Create settings.json for a worktree with trust setting and inherited config.
///
/// This function:
/// 1. Reads the main repo's settings.json (if it exists)
/// 2. Sets `hasTrustDialogAccepted: true` to skip the trust prompt
/// 3. Strips stale per-session identity vars (`LOOM_MAIN_AGENT_PID`,
///    `LOOM_STAGE_ID`, `LOOM_SESSION_ID`) and a stale `LOOM_WORK_DIR` pin
///    from the inherited `env` block
/// 4. Writes the merged result to the worktree
///
/// Note: We deliberately do NOT write `permissions.defaultMode` here. The
/// resolved permission mode (stage-type default + plan override + stage
/// override) lives in `settings.local.json` written by `sandbox::write_settings`
/// using `apply_default_mode`. Writing it here would race the sandbox-merge
/// step and undercut the resolved value. See finding #5 (option 2).
///
/// This creates the base settings.json. The hooks system later merges in
/// session-specific hooks via setup_worktree_hooks().
fn create_worktree_settings(
    main_settings: &Path,
    worktree_settings: &Path,
    worktree_path: &Path,
) -> Result<()> {
    // Start with main repo settings or empty object
    let mut settings: Value = if main_settings.exists() {
        let content = std::fs::read_to_string(main_settings)
            .with_context(|| "Failed to read main repo settings.json")?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Ensure settings is an object
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json must be a JSON object"))?;

    // Set hasTrustDialogAccepted to skip the trust prompt
    obj.insert("hasTrustDialogAccepted".to_string(), json!(true));

    // Ensure a permissions object exists so the `.work` allow-list block
    // below can attach to it. We intentionally do NOT seed `defaultMode`
    // here — that's the sandbox-resolved value's job (see fn-level docs).
    obj.entry("permissions").or_insert_with(|| json!({}));

    // Scrub the copied env block: per-session identity (LOOM_MAIN_AGENT_PID,
    // LOOM_STAGE_ID, LOOM_SESSION_ID) is set dynamically by the wrapper
    // script, so any inherited value is stale. A LOOM_WORK_DIR pin naming a
    // now-deleted .work/ is scrubbed alongside — see scrub_stale_work_dir_env.
    crate::fs::permissions::scrub_session_identity_env(&mut settings);
    crate::fs::permissions::scrub_stale_work_dir_env(&mut settings);

    // Resolve the .work symlink to its absolute target path and add permissions.
    // In worktrees, .work is a symlink to ../../.work (the main repo's .work/).
    // Claude Code resolves symlinks before checking permission patterns, so the
    // relative Read(.work/**) pattern from the main repo's settings doesn't match
    // the resolved absolute path. Adding the resolved path here ensures agents
    // can read/write .work state files without permission prompts.
    //
    // IMPORTANT: Claude Code requires the // prefix for absolute filesystem paths.
    // A single / means "relative to project root", NOT absolute. See:
    // https://code.claude.com/docs/en/permissions.md
    let work_link = worktree_path.join(".work");
    if work_link.exists() || work_link.is_symlink() {
        if let Ok(resolved) = work_link.canonicalize() {
            let resolved_str = resolved.to_string_lossy();

            // Collect the permissions to add
            // Use / prefix on absolute paths for Claude Code's // convention
            // The `Write(...)` rule below is NOT a typo for `Edit(...)` and
            // must stay `Write(`. Claude Code's file permission check
            // consults only `Edit(path)` rules, so `Write(` is parsed and
            // then silently ignored — this grant is inert. Converting it to
            // `Edit(` would RESTORE a broad `Edit(/{resolved}/**)` grant over
            // the whole resolved `.work` root that was deliberately narrowed
            // elsewhere (S-1, see `sandbox/settings.rs`) because it exposed
            // `.work/admin.token` and `.work/user.token` to a sandboxed
            // worktree agent — a daemon RPC privilege escalation.
            let work_perms = vec![
                format!("Read(/{}/**)", resolved_str),
                format!("Write(/{}/**)", resolved_str),
                format!("Read(/{}/signals/**)", resolved_str),
                format!("Read(/{}/config.toml)", resolved_str),
                format!("Read(/{}/handoffs/**)", resolved_str),
            ];

            // Get or create the allow array within permissions
            let permissions = settings
                .as_object_mut()
                .and_then(|o| o.get_mut("permissions"))
                .and_then(|p| p.as_object_mut());

            if let Some(perms_obj) = permissions {
                let allow = perms_obj
                    .entry("allow")
                    .or_insert_with(|| json!([]))
                    .as_array_mut();

                if let Some(allow_arr) = allow {
                    let existing: HashSet<String> = allow_arr
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();

                    for perm in work_perms {
                        if !existing.contains(&perm) {
                            allow_arr.push(json!(perm));
                        }
                    }
                }
            }
        }
    }

    // Write the merged settings
    let content =
        serde_json::to_string_pretty(&settings).with_context(|| "Failed to serialize settings")?;
    std::fs::write(worktree_settings, content)
        .with_context(|| "Failed to write worktree settings.json")?;

    Ok(())
}

/// Configure hooks for a worktree with session context
///
/// This adds Claude Code hooks to the worktree's .claude/settings.json.
/// Hooks enable:
/// - Auto-handoff on PreCompact (context exhaustion)
/// - Learning protection via Stop hook
/// - Session lifecycle tracking
///
/// Session identity (stage/session IDs) is NOT written here: hooks read it
/// from the process environment exported by the session wrapper script.
pub fn setup_worktree_hooks(
    worktree_path: &Path,
    work_dir: &Path,
    hooks_dir: &Path,
    permission_mode: PermissionMode,
) -> Result<()> {
    // Canonicalize work_dir to absolute path so hooks work regardless of
    // Claude Code's current working directory. This fixes "spawn /bin/sh ENOENT"
    // errors that occur when hooks run from a deleted/changed directory.
    let absolute_work_dir = work_dir
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(work_dir));

    let config = HooksConfig::new(hooks_dir.to_path_buf(), absolute_work_dir, permission_mode);

    setup_hooks_for_worktree(worktree_path, &config).with_context(|| {
        format!(
            "Failed to setup hooks for worktree: {}",
            worktree_path.display()
        )
    })
}

/// Append a pattern to a git `info/exclude` file, creating it if absent.
///
/// Idempotent: skips the write when the pattern is already present.
fn add_to_gitignore_exclude(git_dir: &Path, pattern: &str) -> Result<()> {
    let info_dir = git_dir.join("info");
    std::fs::create_dir_all(&info_dir)
        .with_context(|| format!("Failed to create {}", info_dir.display()))?;
    let exclude_path = info_dir.join("exclude");

    if exclude_path.exists() {
        let content = std::fs::read_to_string(&exclude_path)
            .with_context(|| format!("Failed to read {}", exclude_path.display()))?;
        if content.lines().any(|line| line.trim() == pattern) {
            return Ok(());
        }
        let newline = if content.ends_with('\n') { "" } else { "\n" };
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&exclude_path)
            .with_context(|| format!("Failed to open {} for append", exclude_path.display()))?;
        file.write_all(format!("{newline}{pattern}\n").as_bytes())
            .with_context(|| "Failed to append to gitignore exclude")?;
    } else {
        std::fs::write(
            &exclude_path,
            format!("# loom: per-worktree generated paths\n{pattern}\n"),
        )
        .with_context(|| format!("Failed to create {}", exclude_path.display()))?;
    }

    Ok(())
}

/// Patterns loom excludes from git's view of every worktree: the previous
/// session's `.claude/settings.local.json`, plus loom's own runtime output
/// (the memory spool and the derived context cache — see
/// `is_worktree_scaffold_path`). Deliberately NOT a blanket `.loom/`: a
/// project may legitimately track `.loom/config.toml`, and excluding the
/// whole directory would silently hide it from `git status`. These match
/// the patterns this repository's own `.gitignore` hand-lists for the same
/// reason.
const WORKTREE_EXCLUDE_PATTERNS: &[&str] =
    &[".claude/settings.local.json", SPOOL_RELPATH, ".loom/cache/"];

/// Write [`WORKTREE_EXCLUDE_PATTERNS`] into `git_dir`'s `info/exclude`.
fn add_worktree_exclude_patterns(git_dir: &Path) -> Result<()> {
    for pattern in WORKTREE_EXCLUDE_PATTERNS {
        add_to_gitignore_exclude(git_dir, pattern)?;
    }
    Ok(())
}

/// Exclude loom's own runtime paths from git's view of a worktree.
///
/// Writes to the repository's COMMON `.git/info/exclude`, not a per-worktree
/// file. Git worktrees each get their own metadata directory at
/// `<repo>/.git/worktrees/<stage-id>/`, but `info/exclude` is not among the
/// files git treats as per-worktree there — `git status` always resolves
/// `info/exclude` to the common git dir (`<repo>/.git/info/exclude`), shared
/// by every worktree and the main checkout alike. A previous version of this
/// function wrote to `.git/worktrees/<stage-id>/info/exclude`, believing it
/// acted as an exclude file scoped to that worktree; git never reads that
/// path, so the write was silently inert. Writing to the common dir instead
/// means this function and [`add_settings_local_to_main_gitignore`] now
/// target the same file — kept as two entry points because their callers
/// (worktree creation vs. main-repo knowledge stages) are otherwise
/// unrelated, and future patterns may diverge between them.
pub fn add_settings_local_to_worktree_gitignore(repo_root: &Path) -> Result<()> {
    add_worktree_exclude_patterns(&repo_root.join(".git"))
}

/// Exclude loom's own runtime paths from the main repo's git exclude.
///
/// Used for knowledge stages that run in the main repo without a dedicated worktree.
pub fn add_settings_local_to_main_gitignore(repo_root: &Path) -> Result<()> {
    add_worktree_exclude_patterns(&repo_root.join(".git"))
}

/// Remove worktree-specific settings and symlinks
///
/// Called during worktree removal to clean up:
/// - .work symlink
/// - .claude directory (or legacy symlink)
/// - root CLAUDE.md symlink
///
/// Unconditional: it removes `.claude/` and root `CLAUDE.md` whenever they
/// exist, whatever planted them, so it is only safe ahead of `git worktree
/// remove --force` (its sole production caller is the spawn-failure path in
/// `orchestrator/core/stage_executor.rs`). The non-forced removal path must
/// use `git::cleanup::remove_worktree_scaffold` instead, which removes only
/// what loom planted (and leaves anything git tracks alone).
pub fn cleanup_worktree_settings(worktree_path: &Path) {
    // Remove the .work symlink first to avoid issues
    let work_link = worktree_path.join(".work");
    if work_link.exists() || work_link.is_symlink() {
        std::fs::remove_file(&work_link).ok(); // Ignore errors
    }

    // Remove the .claude directory (it's a real directory now, not a symlink)
    let claude_dir = worktree_path.join(".claude");
    if claude_dir.exists() {
        std::fs::remove_dir_all(&claude_dir).ok(); // Ignore errors
    } else if claude_dir.is_symlink() {
        // Handle legacy symlink case
        std::fs::remove_file(&claude_dir).ok();
    }

    // Remove the root CLAUDE.md symlink
    let root_claude_md = worktree_path.join("CLAUDE.md");
    if root_claude_md.exists() || root_claude_md.is_symlink() {
        std::fs::remove_file(&root_claude_md).ok(); // Ignore errors
    }
}

#[cfg(test)]
#[path = "tests_settings.rs"]
mod tests;
#[cfg(test)]
#[path = "tests_settings_env.rs"]
mod tests_settings_env;
