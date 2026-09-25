//! Worktree-root entries a sandbox leaves behind without any agent writing
//! them, and the one rule that keeps them out of change listings.
//!
//! Claude Code's sandbox denies writes to a fixed set of shell, git, editor
//! and MCP configuration paths ([`NAMES`]). Where such a path does not exist
//! at the worktree root, the Linux sandbox bind-mounts `/dev/null` over it:
//! inside the sandbox git lists the mount point as an untracked file while
//! `lstat` sees a character device, and on the host the mount point is an
//! empty regular file, there while a sandboxed command runs and sometimes
//! left behind after it. The list holds the names a sandbox is known to
//! leave; a sandbox release that masks another root path adds its name here.
//!
//! [`is_tool_artifact`] is the rule every listing of untracked changes
//! applies. Tracked paths never reach it: a listed name that is tracked at the
//! base or at `HEAD` shows up in the tracked half of a listing, whatever its
//! content, and stays a change.

use std::io::ErrorKind;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;

/// Worktree-root names Claude Code's sandbox protects from writes: its
/// mandatory-deny list of dangerous files and directories.
pub const NAMES: [&str; 11] = [
    ".bashrc",       // bash startup file
    ".bash_profile", // bash login startup file
    ".gitconfig",    // git configuration
    ".gitmodules",   // git submodule configuration
    ".idea",         // JetBrains project settings
    ".mcp.json",     // project MCP server configuration
    ".profile",      // POSIX login shell startup file
    ".ripgreprc",    // ripgrep configuration
    ".vscode",       // VS Code workspace settings
    ".zprofile",     // zsh login startup file
    ".zshrc",        // zsh startup file
];

/// Whether `path`, an untracked entry as git lists it relative to
/// `worktree_root` (an untracked directory may carry a trailing `/`), is a
/// sandbox artifact to leave out of a change listing.
///
/// True only for an entry at the worktree root whose name is in [`NAMES`] and
/// which is a device node (the sandbox's `/dev/null` mount seen from inside),
/// an empty regular file or an empty directory (the mount point seen from the
/// host), or no longer there at all (a mount point removed after git listed
/// it). A listed name holding content, a symlink, a FIFO or a socket is a real
/// change, and so is every entry below the root or under another name. An
/// entry whose type cannot be read is kept.
pub fn is_tool_artifact(worktree_root: &Path, path: &str) -> bool {
    artifact_among(&NAMES, worktree_root, path)
}

/// [`is_tool_artifact`] against `names`, so a test can name a real device
/// node the process cannot create.
fn artifact_among(names: &[&str], worktree_root: &Path, path: &str) -> bool {
    let name = path.strip_suffix('/').unwrap_or(path);
    if !names.contains(&name) {
        return false;
    }
    let entry = worktree_root.join(name);
    match std::fs::symlink_metadata(&entry) {
        Err(error) => error.kind() == ErrorKind::NotFound,
        Ok(metadata) => {
            let kind = metadata.file_type();
            kind.is_char_device()
                || kind.is_block_device()
                || (kind.is_file() && metadata.len() == 0)
                || (kind.is_dir() && is_empty_dir(&entry))
        }
    }
}

fn is_empty_dir(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_or_directory_at_a_listed_name_is_an_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join(".bashrc"), b"").unwrap();
        std::fs::create_dir(root.join(".vscode")).unwrap();

        assert!(is_tool_artifact(root, ".bashrc"));
        assert!(is_tool_artifact(root, ".vscode/"));
    }

    /// Inside the sandbox the name is the `/dev/null` mount; the host's own
    /// `/dev/null` stands in for it.
    #[test]
    fn a_device_node_at_a_listed_name_is_an_artifact() {
        assert!(artifact_among(&["null"], Path::new("/dev"), "null"));
        assert!(!artifact_among(&[".bashrc"], Path::new("/dev"), "null"));
    }

    #[test]
    fn a_listed_name_that_is_gone_is_an_artifact() {
        let temp = tempfile::tempdir().unwrap();
        assert!(is_tool_artifact(temp.path(), ".mcp.json"));
    }

    #[test]
    fn content_at_a_listed_name_is_a_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join(".gitconfig"), b"[user]\n").unwrap();
        std::fs::create_dir_all(root.join(".idea")).unwrap();
        std::fs::write(root.join(".idea/workspace.xml"), b"<x/>").unwrap();
        nix::unistd::mkfifo(&root.join(".zshrc"), nix::sys::stat::Mode::S_IRWXU).unwrap();
        std::os::unix::fs::symlink("/dev/null", root.join(".profile")).unwrap();

        for path in [
            ".gitconfig",
            ".idea/",
            ".idea/workspace.xml",
            ".zshrc",
            ".profile",
        ] {
            assert!(!is_tool_artifact(root, path), "{path}");
        }
    }

    #[test]
    fn an_empty_file_under_an_unlisted_name_is_a_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::write(root.join("notes.txt"), b"").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/.bashrc"), b"").unwrap();

        assert!(!is_tool_artifact(root, "notes.txt"));
        assert!(!is_tool_artifact(root, "sub/.bashrc"));
    }
}
