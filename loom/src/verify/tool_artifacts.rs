//! Entries a sandbox leaves behind without any agent writing them, and the
//! one rule that keeps them out of change listings.
//!
//! Claude Code's sandbox denies writes to a fixed set of shell, git, editor
//! and MCP configuration names ([`NAMES`]). Where such a name does not exist
//! the Linux sandbox bind-mounts `/dev/null` over it: every name at the
//! worktree root, and a name in a directory below it as well (seen:
//! `.mcp.json` in a subdirectory a session's commands had run from; what
//! triggers a mask below the root is not established, so the rule does not
//! depend on it). Inside the sandbox git lists the mount
//! point as an untracked file while `lstat` sees a character device, and on
//! the host the mount point is an empty regular file, there while a sandboxed
//! command runs and sometimes left behind after it. The list holds the names
//! a sandbox is known to leave; a sandbox release that masks another name
//! adds it here.
//!
//! [`is_tool_artifact`] is the rule every listing of untracked changes
//! applies. It matches a name at any depth. Tracked paths never reach it: a
//! listed name that is tracked at the base or at `HEAD` shows up in the
//! tracked half of a listing, whatever its content, and stays a change.

use std::io::ErrorKind;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path};

/// Names Claude Code's sandbox protects from writes, at the worktree root and
/// in directories below it: its mandatory-deny list of dangerous files and
/// directories.
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
/// True only for an entry whose final path component, at any depth, is in
/// [`NAMES`] and which is the `/dev/null` character device (the mount seen
/// from inside), an empty regular file or an empty directory (the mount point
/// seen from the host), or no longer there at all (a mount point removed
/// after git listed it). A listed name holding content, a symlink, a FIFO or
/// a socket is a real change, and so is every entry under another name. A
/// path that is absolute or has a `..` component, and an entry whose type
/// cannot be read, are kept.
pub fn is_tool_artifact(worktree_root: &Path, path: &str) -> bool {
    artifact_among(&NAMES, worktree_root, path)
}

/// [`is_tool_artifact`] against `names`, so a test can name a real device
/// node the process cannot create.
fn artifact_among(names: &[&str], worktree_root: &Path, path: &str) -> bool {
    let relative = path.strip_suffix('/').unwrap_or(path);
    let escapes = Path::new(relative).components().any(|part| {
        matches!(
            part,
            Component::RootDir | Component::Prefix(_) | Component::ParentDir
        )
    });
    let name = relative.rsplit('/').next().unwrap_or(relative);
    if escapes || !names.contains(&name) {
        return false;
    }
    let entry = worktree_root.join(relative);
    match std::fs::symlink_metadata(&entry) {
        Err(error) => error.kind() == ErrorKind::NotFound,
        Ok(metadata) => {
            let kind = metadata.file_type();
            is_null_device(&metadata)
                || (kind.is_file() && metadata.len() == 0)
                || (kind.is_dir() && is_empty_dir(&entry))
        }
    }
}

fn is_null_device(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_char_device()
        && std::fs::metadata("/dev/null")
            .is_ok_and(|null| null.file_type().is_char_device() && metadata.rdev() == null.rdev())
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
        assert!(artifact_among(&["null"], Path::new("/"), "dev/null"));
        assert!(!artifact_among(&[".bashrc"], Path::new("/dev"), "null"));
        assert!(!artifact_among(&["zero"], Path::new("/dev"), "zero"));
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
    fn an_empty_file_or_directory_below_the_root_is_an_artifact() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("a/b/.vscode")).unwrap();
        std::fs::write(root.join("a/.bashrc"), b"").unwrap();

        assert!(is_tool_artifact(root, "a/.bashrc"));
        assert!(is_tool_artifact(root, "a/b/.vscode/"));
        assert!(is_tool_artifact(root, "a/b/.mcp.json"));
    }

    #[test]
    fn content_a_fifo_or_a_symlink_below_the_root_is_a_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("a/.idea")).unwrap();
        std::fs::write(root.join("a/.idea/workspace.xml"), b"<x/>").unwrap();
        std::fs::write(root.join("a/.gitconfig"), b"[user]\n").unwrap();
        nix::unistd::mkfifo(&root.join("a/.zshrc"), nix::sys::stat::Mode::S_IRWXU).unwrap();
        std::os::unix::fs::symlink("/dev/null", root.join("a/.profile")).unwrap();

        for path in [
            "a/.idea/",
            "a/.idea/workspace.xml",
            "a/.gitconfig",
            "a/.zshrc",
            "a/.profile",
        ] {
            assert!(!is_tool_artifact(root, path), "{path}");
        }
    }

    #[test]
    fn an_empty_file_under_an_unlisted_name_is_a_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("notes.txt"), b"").unwrap();
        std::fs::write(root.join("sub/notes.txt"), b"").unwrap();

        assert!(!is_tool_artifact(root, "notes.txt"));
        assert!(!is_tool_artifact(root, "sub/notes.txt"));
    }

    #[test]
    fn a_path_leaving_the_worktree_is_a_change() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("work");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(temp.path().join(".bashrc"), b"").unwrap();
        std::fs::write(root.join(".bashrc"), b"").unwrap();
        let absolute = root.join(".bashrc");

        assert!(!is_tool_artifact(&root, "../.bashrc"));
        assert!(!is_tool_artifact(&root, "sub/../.bashrc"));
        assert!(!is_tool_artifact(&root, absolute.to_str().unwrap()));
    }
}
