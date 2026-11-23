//! Directories for tests that cross a private-root walk
//! (`native_worker::boundary::private_root`), which refuses an ancestor that group or
//! others can write. `tempfile::tempdir` and `fs::create_dir_all` create `0o777 & !umask`
//! directories, so under umask 002 every one of them is refused. These helpers fix the
//! mode instead of reading the process umask, which is process-wide and which the daemon
//! code changes and restores, so no test may set it.

use std::fs::{DirBuilder, Permissions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::Path;

use tempfile::TempDir;

/// A temp directory with mode 0700, whatever the umask.
pub(crate) fn private_tempdir() -> TempDir {
    tempfile::Builder::new()
        .permissions(Permissions::from_mode(0o700))
        .tempdir()
        .expect("a private temp dir")
}

/// `fs::create_dir_all` with mode 0755 for every directory it creates. A umask only
/// removes bits, so no created directory is group or world writable; existing
/// directories are left as they are.
pub(crate) fn create_dirs(path: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o755).create(path)
}

mod tests {
    use std::fs;

    use super::*;

    fn mode(path: &Path) -> u32 {
        fs::symlink_metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn created_directories_have_no_group_or_other_write_bit() {
        let temp = private_tempdir();
        assert_eq!(mode(temp.path()), 0o700);
        let deep = temp.path().join("a/b/c");
        create_dirs(&deep).unwrap();
        for dir in [temp.path().join("a"), temp.path().join("a/b"), deep] {
            assert_eq!(mode(&dir) & 0o022, 0, "{}", dir.display());
        }
        create_dirs(&temp.path().join("a")).expect("an existing directory is accepted");
    }
}
