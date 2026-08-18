//! The address of the working-tree overlay: the `(plan, stage)` pair the CLI
//! reads and writes for the tree in front of the user, as opposed to a real
//! stage's overlay or the immutable base layer. The dirty-tree fallback in
//! `context::refresh::semantic` already shares this same address.

use std::path::Path;

/// Plan name for the graph overlay the CLI reads and writes for the working
/// tree (the dirty-tree `sync` fallback shares it too). Not a real stage:
/// the underscore prefix keeps it out of any plan's namespace.
pub const LOCAL_PLAN_KEY: &str = "_local";

/// Stage name for the working-tree overlay, keyed by the tree's own
/// directory name.
///
/// `.work` is a symlink to the main repository in every worktree, so a fixed
/// stage name would make every worktree's local view read and clobber the
/// SAME overlay file. Keying by the project root's directory name keeps each
/// worktree's working-tree view private to it - but only if the key is
/// canonical. The same directory can be reached by more than one spelling (a
/// relative "." run from the tree's own root, an absolute path reached from a
/// subdirectory, a path threaded through a `..`), and a writer and a reader
/// that reach it by different spellings must still land on the same address,
/// or a write from one is invisible to a read from the other. Canonicalizing
/// before taking the final component makes the address identify the
/// directory, not how the caller happened to spell it: a worktree
/// canonicalizes to `.worktrees/<stage-id>`, the main checkout to the repo
/// directory, so the "keep each worktree private" property this function
/// exists for still holds. A path that cannot be canonicalized (e.g. it does
/// not exist) falls back to its own final component, same as before.
pub fn local_overlay_stage_name(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    match canonical.file_name() {
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

/// Which overlay a retrieval should read on top of the base layer.
/// Constructed by every production caller of `retrieve_for_stage`
/// (`context::retrieve::StageQuery::new`, `commands/knowledge/context.rs`,
/// `orchestrator/signals/retrieval.rs`) — the shared address type that spares
/// each from building its own `(plan, stage)` resolution logic.
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

    /// `/a/b/myrepo` does not exist, so this exercises the
    /// canonicalization-failure fallback (the path as spelled), not the
    /// canonical happy path.
    #[test]
    fn stage_name_falls_back_when_path_does_not_exist() {
        assert_eq!(
            local_overlay_stage_name(Path::new("/a/b/myrepo")),
            "map-myrepo"
        );
    }

    #[test]
    fn stage_name_falls_back_when_no_file_name() {
        assert_eq!(local_overlay_stage_name(Path::new("/")), "map");
    }

    /// THE REGRESSION GUARD: `local_overlay_stage_name` used to key on how a
    /// path was SPELLED rather than on which directory it named, so a "."
    /// run from a tree's root and an absolute path to that same tree (e.g.
    /// entered from a subdirectory) produced two different overlay
    /// addresses - a write from one spelling was invisible to a read from
    /// the other. Two spellings of the SAME directory must always agree.
    #[test]
    fn stage_name_is_canonical_across_spellings() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();

        let direct = local_overlay_stage_name(&root);
        let via_detour = local_overlay_stage_name(&root.join("sub").join(".."));

        assert_eq!(
            direct, via_detour,
            "two spellings of the same directory must resolve to the same overlay address"
        );
    }

    #[test]
    fn local_scope_resolves_to_local_overlay_key() {
        // `/a/b/myrepo` does not exist, so this also exercises the
        // canonicalization-failure fallback, not just the happy path.
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
