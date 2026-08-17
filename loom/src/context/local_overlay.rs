//! The address of the working-tree overlay: the `(plan, stage)` pair the CLI
//! reads and writes for the tree in front of the user, as opposed to a real
//! stage's overlay or the immutable base layer. A future dirty `sync` path is
//! expected to share this same address, but does not yet.

use std::path::Path;

/// Plan name for the graph overlay the CLI reads and writes for the working
/// tree (a future dirty `sync` is expected to share it). Not a real stage:
/// the underscore prefix keeps it out of any plan's namespace.
pub const LOCAL_PLAN_KEY: &str = "_local";

/// Stage name for the working-tree overlay, keyed by the tree's own
/// directory name.
///
/// `.work` is a symlink to the main repository in every worktree, so a fixed
/// stage name would make every worktree's local view read and clobber the
/// SAME overlay file. Keying by the project root's final path component keeps
/// each worktree's working-tree view private to it.
pub fn local_overlay_stage_name(project_root: &Path) -> String {
    match project_root.file_name() {
        Some(name) => format!("map-{}", name.to_string_lossy()),
        None => "map".to_string(),
    }
}

/// The full `(plan, stage)` address of this checkout's working-tree overlay.
pub fn local_overlay_key(project_root: &Path) -> (String, String) {
    (
        LOCAL_PLAN_KEY.to_string(),
        local_overlay_stage_name(project_root),
    )
}

/// Which overlay a retrieval should read on top of the base layer. Not yet
/// constructed by any production caller — this is the shared address type
/// upcoming context-retrieval work is expected to consume instead of each
/// building its own `(plan, stage)` resolution logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayScope {
    /// The working tree in front of the user right now — a CLI or hook query
    /// with no stage named means "read what's on disk here", not "read the
    /// last clean base revision".
    Local,
    /// A real plan stage's own overlay.
    Stage { plan: String, stage: String },
}

impl OverlayScope {
    /// The `(plan, stage)` pair to hand [`crate::context::graph_store::GraphStore::resolved`].
    pub fn resolve(&self, project_root: &Path) -> (String, String) {
        match self {
            OverlayScope::Local => local_overlay_key(project_root),
            OverlayScope::Stage { plan, stage } => (plan.clone(), stage.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_name_uses_directory_name() {
        assert_eq!(
            local_overlay_stage_name(Path::new("/a/b/myrepo")),
            "map-myrepo"
        );
    }

    #[test]
    fn stage_name_falls_back_when_no_file_name() {
        assert_eq!(local_overlay_stage_name(Path::new("/")), "map");
    }

    #[test]
    fn stage_name_falls_back_for_dot_path() {
        assert_eq!(local_overlay_stage_name(Path::new(".")), "map");
    }

    #[test]
    fn local_scope_resolves_to_local_overlay_key() {
        let root = Path::new("/a/b/myrepo");
        assert_eq!(OverlayScope::Local.resolve(root), local_overlay_key(root));
        assert_eq!(
            OverlayScope::Local.resolve(root),
            ("_local".to_string(), "map-myrepo".to_string())
        );
    }

    #[test]
    fn stage_scope_ignores_project_root() {
        let scope = OverlayScope::Stage {
            plan: "p".to_string(),
            stage: "s".to_string(),
        };
        assert_eq!(
            scope.resolve(Path::new("/a/b/myrepo")),
            ("p".to_string(), "s".to_string())
        );
        assert_eq!(
            scope.resolve(Path::new("/other")),
            ("p".to_string(), "s".to_string())
        );
    }
}
