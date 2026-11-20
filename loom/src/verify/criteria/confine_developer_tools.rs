//! Xcode developer-tools roots for the macOS Seatbelt read rules, from the same
//! lookup that puts the developer directories on a confined command's `PATH`
//! ([`DeveloperTools::detect`]), so the two never disagree.

use std::path::PathBuf;

use crate::stage_cache::DeveloperTools;

/// The selected developer directory, widened to Xcode's `Contents` directory
/// (whose sibling paths its frameworks link into), and the SDK when it lies
/// outside it. Empty when the tools are not detected.
pub(super) fn developer_tools() -> Vec<PathBuf> {
    let Some(tools) = DeveloperTools::detect() else {
        return Vec::new();
    };
    let mut roots = Vec::new();
    if let Ok(path) = tools.developer_dir.canonicalize() {
        let contents = path
            .parent()
            .filter(|parent| path.ends_with("Developer") && parent.ends_with("Contents"));
        roots.push(contents.map_or(path.clone(), PathBuf::from));
    }
    if let Ok(sdk) = tools.sdk_root.canonicalize() {
        if !roots.iter().any(|root| sdk.starts_with(root)) {
            roots.push(sdk);
        }
    }
    roots
}
