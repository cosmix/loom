//! The address of the working-tree overlay: the `(plan, stage)` pair the CLI
//! reads and writes for the tree in front of the user, as opposed to a real
//! stage's overlay or the immutable base layer. The dirty-tree fallback in
//! `context::refresh::semantic` already shares this same address.

use sha2::{Digest, Sha256};
use std::path::Path;

/// Plan name for the graph overlay the CLI reads and writes for the working
/// tree (the dirty-tree `sync` fallback shares it too). Not a real stage:
/// the underscore prefix keeps it out of any plan's namespace.
pub const LOCAL_PLAN_KEY: &str = "_local";

/// Stage name for the working-tree overlay, keyed by the full checkout path.
///
/// Worktrees share `.loom/work`, so even equal-basename checkouts need distinct
/// addresses. Canonicalization makes alternate spellings of an existing root
/// agree. If it fails, the exact path spelling is hashed instead; callers must
/// use the same spelling until the root can be canonicalized.
pub fn local_overlay_stage_name(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
    format!("map-{}", hex::encode(digest))
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
    use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
    use std::process::Command;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn create_worktrees(root: &Path) {
        git(root, &["init", "repo"]);
        let repo = root.join("repo");
        git(
            &repo,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "--allow-empty",
                "-m",
                "base",
            ],
        );
        std::fs::create_dir_all(root.join("a")).unwrap();
        std::fs::create_dir_all(root.join("b")).unwrap();
        git(&repo, &["worktree", "add", "--detach", "../a/shared"]);
        git(&repo, &["worktree", "add", "--detach", "../b/shared"]);
    }

    #[test]
    fn equal_basename_worktrees_isolate_overlays_in_shared_store() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path();
        create_worktrees(root);
        let first = local_overlay_key(&root.join("a/shared"));
        let second = local_overlay_key(&root.join("b/shared"));
        assert_ne!(first, second);
        let store = GraphStore::new(&root.join("cache"), &root.join("work"));
        let mut layer = GraphLayer::default();
        layer.files.insert(
            "src.rs".into(),
            FileEntry {
                content_hash: "first edit".into(),
                ..Default::default()
            },
        );
        store.save_overlay(&first.0, &first.1, &layer).unwrap();
        assert!(store.load_overlay(&second.0, &second.1).unwrap().is_none());
        layer.files.get_mut("src.rs").unwrap().content_hash = "second edit".into();
        store.save_overlay(&second.0, &second.1, &layer).unwrap();
        assert_eq!(
            store
                .load_overlay(&first.0, &first.1)
                .unwrap()
                .unwrap()
                .files["src.rs"]
                .content_hash,
            "first edit"
        );
        assert_eq!(
            store
                .load_overlay(&second.0, &second.1)
                .unwrap()
                .unwrap()
                .files["src.rs"]
                .content_hash,
            "second edit"
        );
    }

    #[test]
    fn stage_name_falls_back_when_path_does_not_exist() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("a/myrepo");
        assert_eq!(
            local_overlay_stage_name(&path),
            local_overlay_stage_name(&path)
        );
        assert_ne!(
            local_overlay_stage_name(&path),
            local_overlay_stage_name(&temp.path().join("b/myrepo"))
        );
    }

    #[test]
    fn stage_name_handles_root_path() {
        assert!(local_overlay_stage_name(Path::new("/")).starts_with("map-"));
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
        assert_eq!(OverlayScope::Local.resolve(root).0, "_local");
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
