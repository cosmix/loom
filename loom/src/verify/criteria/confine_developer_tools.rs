//! Xcode developer-tools root for the macOS Seatbelt read rules.

use std::path::PathBuf;
use std::process::Command;

/// The Xcode Developer directory `xcode-select` reports, resolved once.
pub(super) fn developer_tools() -> &'static Option<PathBuf> {
    static ROOT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let mut command = Command::new("/usr/bin/xcode-select");
        command.env_clear().arg("--print-path");
        let output = crate::process::run_bounded(&mut command, std::time::Duration::from_secs(2))
            .ok()?
            .completed()?;
        if !output.status.success() {
            return None;
        }
        let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
        let path = path
            .is_absolute()
            .then(|| path.canonicalize().ok())
            .flatten()?;
        // Xcode's selected Developer directory links frameworks in sibling Contents paths.
        Some(
            if path.file_name().is_some_and(|name| name == "Developer")
                && path
                    .parent()
                    .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "Contents"))
            {
                path.parent()?.to_path_buf()
            } else {
                path
            },
        )
    })
}
