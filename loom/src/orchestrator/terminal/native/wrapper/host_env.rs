//! The host facts a session wrapper exports so loom's hooks run only
//! executables sessions cannot write: `LOOM_SCRATCH_DIR` (where the CLI stages
//! relay tickets), `LOOM_BIN` (the daemon's own verified binary) and
//! `LOOM_HOOK_PATH` (the daemon's PATH minus every session-writable root).
//! `native::launch` resolves them; this module renders them and holds the
//! rules they answer to: which binary may be `LOOM_BIN`, and which
//! directories a capsule denies writing because hooks run what they hold.

use anyhow::{bail, Context, Result};
use shell_escape::escape;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use super::CONTINUATION;

/// Mode bits that let anyone but the owner write a file.
const GROUP_OR_WORLD_WRITE: u32 = 0o022;

/// The wrapper's host exports; the default renders nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WrapperHostEnv {
    /// `<scratch_root>/<session-id>`, exported as `LOOM_SCRATCH_DIR`.
    pub scratch_dir: Option<PathBuf>,
    /// The verified loom binary, exported as `LOOM_BIN`.
    pub loom_bin: Option<PathBuf>,
    /// The filtered PATH entries, exported colon-joined as `LOOM_HOOK_PATH`.
    pub hook_path: Vec<PathBuf>,
}

impl WrapperHostEnv {
    /// Shell-escaped `exec env` assignments, one continuation line each, in
    /// the shape of the wrapper's other exports.
    pub(super) fn render(&self) -> String {
        let hook_path = (!self.hook_path.is_empty()).then(|| {
            self.hook_path
                .iter()
                .map(|dir| dir.display().to_string())
                .collect::<Vec<_>>()
                .join(":")
        });
        [
            (
                "LOOM_SCRATCH_DIR",
                self.scratch_dir
                    .as_ref()
                    .map(|dir| dir.display().to_string()),
            ),
            (
                "LOOM_BIN",
                self.loom_bin.as_ref().map(|bin| bin.display().to_string()),
            ),
            ("LOOM_HOOK_PATH", hook_path),
        ]
        .into_iter()
        .filter_map(|(name, value)| {
            let assignment = escape(format!("{name}={}", value?).into());
            Some(format!("    {assignment} {CONTINUATION}"))
        })
        .collect()
    }
}

/// `exe` accepted as `LOOM_BIN`: canonical, a regular file owned by the
/// operator (`uid`) or root, writable by its owner alone, and outside every
/// one of `writable_roots` (`sandbox::control_surfaces::session_writable_roots`).
/// Every loom hook runs this binary, so whoever could replace it would run
/// code outside the sandbox.
pub(crate) fn accepted_loom_bin(
    exe: &Path,
    uid: u32,
    writable_roots: &[PathBuf],
) -> Result<PathBuf> {
    let canonical = exe
        .canonicalize()
        .with_context(|| format!("cannot resolve the loom binary {}", exe.display()))?;
    let metadata = std::fs::metadata(&canonical)
        .with_context(|| format!("cannot stat the loom binary {}", canonical.display()))?;
    if !metadata.is_file() {
        bail!("LOOM_BIN {} is not a regular file", canonical.display());
    }
    check_owner_and_mode(&canonical, metadata.uid(), metadata.mode(), uid)?;
    let writable = writable_roots
        .iter()
        .map(|root| super::absolute(root))
        .find(|root| canonical.starts_with(root));
    if let Some(root) = writable {
        bail!(
            "LOOM_BIN {} lies under {}, which loom sessions can write",
            canonical.display(),
            root.display()
        );
    }
    Ok(canonical)
}

/// The ownership and mode rule `accepted_loom_bin` applies to already-read
/// metadata: owned by `operator` or root, and neither group- nor
/// world-writable.
fn check_owner_and_mode(path: &Path, owner: u32, mode: u32, operator: u32) -> Result<()> {
    if owner != operator && owner != 0 {
        bail!(
            "LOOM_BIN {} is owned by uid {owner}, neither the operator (uid {operator}) nor root",
            path.display()
        );
    }
    if mode & GROUP_OR_WORLD_WRITE != 0 {
        bail!(
            "LOOM_BIN {} is group- or world-writable (mode {:o}); fix with `chmod go-w {}`",
            path.display(),
            mode & 0o7777,
            path.display()
        );
    }
    Ok(())
}

/// The directories holding executables loom's hooks run that a capsule
/// denies writing: `dirname(loom_bin)` and each `hook_path` entry the
/// operator (`uid`) owns. Root-owned entries are left out: no session can
/// write them anyway.
pub(crate) fn operator_executable_dirs(
    loom_bin: &Path,
    hook_path: &[PathBuf],
    uid: u32,
) -> Vec<PathBuf> {
    let owned = |dir: &&PathBuf| std::fs::metadata(dir).is_ok_and(|metadata| metadata.uid() == uid);
    let mut dirs: Vec<PathBuf> = loom_bin
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    dirs.extend(hook_path.iter().filter(owned).cloned());
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// A `loom` file in `dir` with `mode`, and the uid that owns it.
    fn binary(dir: &Path, mode: u32) -> (PathBuf, u32) {
        let path = dir.join("loom");
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        let uid = std::fs::metadata(&path).unwrap().uid();
        (path, uid)
    }

    #[test]
    fn an_operator_owned_0755_binary_outside_every_writable_root_is_accepted() {
        let temp = TempDir::new().unwrap();
        let (bin, uid) = binary(temp.path(), 0o755);
        let elsewhere = TempDir::new().unwrap();

        let accepted = accepted_loom_bin(&bin, uid, &[elsewhere.path().to_path_buf()]).unwrap();

        assert_eq!(accepted, bin.canonicalize().unwrap());
    }

    #[test]
    fn a_group_or_world_writable_binary_is_refused() {
        for mode in [0o775, 0o757] {
            let temp = TempDir::new().unwrap();
            let (bin, uid) = binary(temp.path(), mode);

            let error = accepted_loom_bin(&bin, uid, &[]).unwrap_err();

            let message = format!("{error:#}");
            assert!(message.contains("writable"), "{message}");
            assert!(
                message.contains(&format!(
                    "chmod go-w {}",
                    bin.canonicalize().unwrap().display()
                )),
                "{message}"
            );
        }
    }

    #[test]
    fn a_binary_under_a_session_writable_root_is_refused() {
        let temp = TempDir::new().unwrap();
        let (bin, uid) = binary(temp.path(), 0o755);

        let error = accepted_loom_bin(&bin, uid, &[temp.path().to_path_buf()]).unwrap_err();

        assert!(
            format!("{error:#}").contains("loom sessions can write"),
            "{error:#}"
        );
    }

    #[test]
    fn a_directory_a_missing_path_or_a_stranger_owned_binary_is_refused() {
        let temp = TempDir::new().unwrap();
        let (bin, uid) = binary(temp.path(), 0o755);
        assert!(accepted_loom_bin(temp.path(), uid, &[]).is_err());
        assert!(accepted_loom_bin(&temp.path().join("missing"), uid, &[]).is_err());
        if uid != 0 {
            assert!(accepted_loom_bin(&bin, uid.wrapping_add(1), &[]).is_err());
        }
    }

    #[test]
    fn root_or_the_operator_may_own_the_binary_and_nobody_else() {
        let path = Path::new("/opt/loom/bin/loom");
        assert!(check_owner_and_mode(path, 0, 0o755, 1000).is_ok());
        assert!(check_owner_and_mode(path, 1000, 0o755, 1000).is_ok());
        assert!(check_owner_and_mode(path, 4242, 0o755, 1000).is_err());
        assert!(check_owner_and_mode(path, 0, 0o775, 1000).is_err());
    }

    #[test]
    fn a_symlink_to_a_writable_root_still_matches_the_binary_under_the_real_path() {
        let real_root = TempDir::new().unwrap();
        let (bin, uid) = binary(real_root.path(), 0o755);
        let symlink_parent = TempDir::new().unwrap();
        let symlinked_root = symlink_parent.path().join("root-symlink");
        std::os::unix::fs::symlink(real_root.path(), &symlinked_root).unwrap();

        let error = accepted_loom_bin(&bin, uid, &[symlinked_root]).unwrap_err();

        assert!(
            format!("{error:#}").contains("loom sessions can write"),
            "{error:#}"
        );
    }

    #[test]
    fn executable_dirs_are_the_binarys_directory_and_the_operator_owned_hook_path() {
        let temp = TempDir::new().unwrap();
        let (bin, uid) = binary(temp.path(), 0o755);
        let owned = TempDir::new().unwrap();
        let hook_path = [owned.path().to_path_buf()];

        let dirs = operator_executable_dirs(&bin, &hook_path, uid);
        assert_eq!(
            dirs,
            [temp.path().to_path_buf(), owned.path().to_path_buf()]
        );

        let stranger = operator_executable_dirs(&bin, &hook_path, uid.wrapping_add(1));
        assert_eq!(stranger, [temp.path().to_path_buf()]);
    }
}
