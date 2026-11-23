use std::path::{Component, Path};

/// A name such as `foo..bar.rs` is an ordinary component. Only a component
/// equal to `..` moves up a directory.
pub(super) fn has_parent_component(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|component| component == Component::ParentDir)
}
