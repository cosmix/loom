//! The loom-owned list of permissions approved in earlier sessions (owner
//! decision 8 in `doc/plans/PLAN-loom-state-confinement.md`).
//!
//! Filled where the fold-back runs (`sync.rs`) and rendered into every
//! session capsule's `permissions.allow`. Every rule passes the
//! control-surface filter on the way in AND on the way out: the list lives
//! under the state root, which checkout-rooted sessions can still write until
//! the phase-3 deny rules land, so a rule planted there directly must not
//! reach a capsule either.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use crate::sandbox::control_surfaces::ControlSurfaces;

/// Private, like every other state-root subdirectory.
const DIR_MODE: u32 = 0o700;

/// `<work_dir>/permissions/approved.json`.
pub(crate) fn approved_path(work_dir: &Path) -> PathBuf {
    work_dir.join("permissions").join("approved.json")
}

/// The approved rules that name no control surface, in stored (sorted) order.
///
/// A missing list is empty. An unreadable or malformed one is logged and
/// treated as empty: it only ever adds convenience grants, so it must never
/// block a spawn.
pub(crate) fn approved_rules(work_dir: &Path, surfaces: &ControlSurfaces) -> Vec<String> {
    let path = approved_path(work_dir);
    if !path.exists() {
        return Vec::new();
    }
    match crate::fs::locking::locked_read(&path).and_then(|content| parse(&content)) {
        Ok(rules) => rules
            .into_iter()
            .filter(|rule| !surfaces.names(rule))
            .collect(),
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "ignoring an unreadable approved-permissions list"
            );
            Vec::new()
        }
    }
}

/// Merge every rule of `rules` that names no control surface into the list,
/// returning how many were new. The list stays sorted and free of duplicates.
pub(crate) fn record_approved(
    work_dir: &Path,
    rules: &[String],
    surfaces: &ControlSurfaces,
) -> Result<usize> {
    let candidates: Vec<&String> = rules.iter().filter(|rule| !surfaces.names(rule)).collect();
    if candidates.is_empty() {
        return Ok(0);
    }
    let path = approved_path(work_dir);
    let dir_relpath = path
        .strip_prefix(work_dir)
        .context("approved-permissions path is not under the work dir")?
        .parent()
        .context("approved-permissions path has no parent")?;
    ensure_private_dir(work_dir, dir_relpath)?;
    let mut added = 0;
    crate::fs::locking::locked_update(&path, |content| {
        let mut current = if content.trim().is_empty() {
            Vec::new()
        } else {
            parse(&content)?
        };
        for rule in candidates {
            if !current.contains(rule) {
                current.push(rule.clone());
                added += 1;
            }
        }
        current.sort();
        serde_json::to_string_pretty(&json!({ "allow": current }))
            .context("failed to serialize the approved-permissions list")
    })?;
    Ok(added)
}

/// Record what a finished session approved into the list under the state
/// root `main_repo_path` resolves to. Best effort: a failure is logged, never
/// raised, because the fold-back that calls this must still run.
pub(crate) fn record_fold_back(main_repo_path: &Path, rules: &[String]) {
    let Some(work_dir) = super::state_root::resolve_state_root(main_repo_path) else {
        return;
    };
    let scratch_root = crate::relay::scratch_root_from_env().ok();
    let hooks_dirs: Vec<PathBuf> = crate::hooks::find_hooks_dir().into_iter().collect();
    let home = dirs::home_dir();
    let surfaces = ControlSurfaces::new(
        &work_dir,
        scratch_root.as_deref(),
        &hooks_dirs,
        home.as_deref(),
    );
    if let Err(error) = record_approved(&work_dir, rules, &surfaces) {
        tracing::warn!(%error, "failed to record approved permissions");
    }
}

fn parse(content: &str) -> Result<Vec<String>> {
    let value: Value =
        serde_json::from_str(content).context("the approved-permissions list is not JSON")?;
    let allow = value
        .get("allow")
        .and_then(Value::as_array)
        .context("the approved-permissions list has no `allow` array")?;
    Ok(allow
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect())
}

/// Create (or tighten) `<work_dir>/<relpath>` to [`DIR_MODE`], refusing to
/// follow a symlink planted anywhere along the way or at the leaf: sessions in
/// the main checkout can still write under the state root until the phase-3
/// deny rules land, so a symlink dropped where this directory belongs must
/// not redirect the daemon's `chmod` or write through it. Reopening the
/// created directory with `O_NOFOLLOW` and `fchmod`-ing the resulting
/// descriptor — never the path — also tightens one left over from an earlier,
/// more permissive version.
fn ensure_private_dir(work_dir: &Path, relpath: &Path) -> Result<()> {
    std::fs::create_dir_all(work_dir)
        .with_context(|| format!("failed to create {}", work_dir.display()))?;
    let dirfd = crate::fs::safe_fs::safe_open_dirfd(work_dir)
        .with_context(|| format!("failed to open {}", work_dir.display()))?;
    #[allow(clippy::unnecessary_cast)] // mode_t is u32 on Linux but u16 on macOS
    let mode = DIR_MODE as libc::mode_t;
    crate::fs::safe_fs::safe_create_dir_all_in_workdir(dirfd.as_raw_fd(), relpath, mode)
        .with_context(|| format!("failed to create {}", work_dir.join(relpath).display()))?;
    let full = work_dir.join(relpath);
    let dir_fd = crate::fs::safe_fs::safe_open_dirfd(&full)
        .with_context(|| format!("failed to open {}", full.display()))?;
    std::fs::File::from(dir_fd)
        .set_permissions(std::fs::Permissions::from_mode(DIR_MODE))
        .with_context(|| format!("failed to restrict {}", full.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn surfaces(work_dir: &Path) -> ControlSurfaces {
        ControlSurfaces::new(
            work_dir,
            Some(Path::new("/scratch-root")),
            &[PathBuf::from("/opt/hooks")],
            Some(Path::new("/home/op")),
        )
    }

    fn rules(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn records_and_reads_back_sorted_without_duplicates() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join("work");
        let surfaces = surfaces(&work_dir);

        let first = rules(&["WebFetch(domain:docs.rs)", "Bash(cargo test:*)"]);
        assert_eq!(record_approved(&work_dir, &first, &surfaces).unwrap(), 2);
        let again = rules(&["Bash(cargo test:*)"]);
        assert_eq!(record_approved(&work_dir, &again, &surfaces).unwrap(), 0);

        assert_eq!(
            approved_rules(&work_dir, &surfaces),
            rules(&["Bash(cargo test:*)", "WebFetch(domain:docs.rs)"])
        );
        let dir = approved_path(&work_dir).parent().unwrap().to_path_buf();
        let mode = std::fs::metadata(dir).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, DIR_MODE);
    }

    #[test]
    fn control_surface_rules_are_never_recorded() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join("work");
        let mut dropped = rules(&[
            "Edit(.loom/work/handoffs/**)",
            "Edit(.work/memory/**)",
            "Edit(.claude/settings.json)",
            "Edit(.worktrees/s1/**)",
            "Edit(//scratch-root/session-1/**)",
            "Edit(//opt/hooks/loom-relay.sh)",
            "Edit(~/.loom/config.toml)",
        ]);
        dropped.push(format!("Edit(/{}/signals/**)", work_dir.display()));

        assert_eq!(
            record_approved(&work_dir, &dropped, &surfaces(&work_dir)).unwrap(),
            0
        );
        assert!(!approved_path(&work_dir).exists());
    }

    #[test]
    fn a_planted_control_surface_rule_is_filtered_on_the_way_out() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join("work");
        std::fs::create_dir_all(work_dir.join("permissions")).unwrap();
        let planted = json!({ "allow": ["Bash(cargo test:*)", "Edit(.claude/**)"] });
        std::fs::write(approved_path(&work_dir), planted.to_string()).unwrap();

        assert_eq!(
            approved_rules(&work_dir, &surfaces(&work_dir)),
            rules(&["Bash(cargo test:*)"])
        );
    }

    #[test]
    fn a_malformed_list_reads_as_empty_and_is_never_overwritten() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join("work");
        std::fs::create_dir_all(work_dir.join("permissions")).unwrap();
        std::fs::write(approved_path(&work_dir), "not json").unwrap();
        let surfaces = surfaces(&work_dir);

        assert!(approved_rules(&work_dir, &surfaces).is_empty());
        assert!(record_approved(&work_dir, &rules(&["Bash(ls:*)"]), &surfaces).is_err());
        assert_eq!(
            std::fs::read_to_string(approved_path(&work_dir)).unwrap(),
            "not json"
        );
    }

    #[test]
    fn record_approved_refuses_a_symlinked_permissions_directory() {
        let temp = TempDir::new().unwrap();
        let work_dir = temp.path().join("work");
        std::fs::create_dir_all(&work_dir).unwrap();
        let attacker_target = temp.path().join("attacker");
        std::fs::create_dir_all(&attacker_target).unwrap();
        std::os::unix::fs::symlink(&attacker_target, work_dir.join("permissions")).unwrap();

        let error = record_approved(
            &work_dir,
            &rules(&["Bash(cargo test:*)"]),
            &surfaces(&work_dir),
        )
        .expect_err("a symlinked permissions directory must be refused");

        assert!(format!("{error:#}").contains("permissions"), "{error:#}");
        assert!(
            std::fs::read_dir(&attacker_target)
                .unwrap()
                .next()
                .is_none(),
            "must not write through the symlinked permissions directory"
        );
    }
}
