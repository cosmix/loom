//! Provisioning's Unix-socket denials, rendered on any host: a cache root
//! outside `<cache base>/loom/stage-cache` still keeps every stage cache and
//! base-run cache below this process's cache base out of reach.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{process_cache_bases, profile, regex_prefix, CargoScratch, Grants, NetworkAccess};
use crate::stage_cache::StageCache;

/// The rule denying a connect to any Unix socket below `base`.
fn socket_deny(base: &Path) -> String {
    let prefix = regex_prefix(base).unwrap();
    format!("(deny network-outbound (remote unix-socket (path-regex #\"{prefix}\")))")
}

/// The rule letting a command reach the `h.sock` server of `root`.
fn socket_allow(root: &Path) -> String {
    let socket = serde_json::to_string(&root.join("h.sock").to_string_lossy()).unwrap();
    format!("(allow network-outbound (remote unix-socket (path-literal {socket})))")
}

/// A tree and a cache root laid out as the impact base comparison's
/// temporary one, outside this process's cache base.
struct Layout {
    _root: TempDir,
    tree: PathBuf,
    cache_root: PathBuf,
}

fn layout() -> Layout {
    let root = TempDir::new().unwrap();
    let canonical = root.path().canonicalize().unwrap();
    let tree = canonical.join("worktree");
    let cache_root = canonical.join("loom-impact-base-x/cache/loom/stage-cache/name");
    for directory in [&tree, &cache_root] {
        std::fs::create_dir_all(directory).unwrap();
    }
    Layout {
        _root: root,
        tree,
        cache_root,
    }
}

fn render(layout: &Layout, network: NetworkAccess) -> String {
    let mut original = Command::new("/bin/sh");
    original.current_dir(&layout.tree);
    let scratch = CargoScratch::prepare(&original, &layout.tree, false, false).unwrap();
    let grants = Grants {
        cache_root: Some(&layout.cache_root),
        ..Grants::default()
    };
    profile(&original, &layout.tree, &scratch, network, &grants).unwrap()
}

#[test]
fn the_process_cache_bases_are_the_stage_cache_and_base_run_parents() {
    let cache = StageCache::resolve_from_process(Path::new("/repo"), "stage").unwrap();
    let stage_caches = cache.root().parent().unwrap();
    let base_runs = stage_caches.parent().unwrap().join("stage-base");
    assert!(
        stage_caches.ends_with("loom/stage-cache"),
        "{stage_caches:?}"
    );
    assert_eq!(
        process_cache_bases().unwrap(),
        vec![stage_caches.to_path_buf(), base_runs]
    );
}

#[test]
fn provisioning_with_a_base_cache_denies_every_process_cache_base_before_its_own_socket() {
    let layout = layout();
    let profile = render(&layout, NetworkAccess::Provision);
    let allow = profile
        .find(&socket_allow(&layout.cache_root))
        .expect("the granted root's own socket stays reachable");
    let granted_parent = layout.cache_root.parent().unwrap().to_path_buf();
    let mut bases = process_cache_bases().unwrap();
    bases.push(granted_parent);
    for base in &bases {
        let deny = profile.find(&socket_deny(base));
        assert!(
            deny.is_some_and(|deny| deny < allow),
            "a socket deny for {base:?} must precede the granted socket in {profile}"
        );
    }
}

#[test]
fn a_denied_network_renders_no_provisioning_rules() {
    let layout = layout();
    let profile = render(&layout, NetworkAccess::Denied);
    assert!(
        !profile.contains("(allow network-outbound)"),
        "a denied network has no blanket outbound allow: {profile}"
    );
    let stage_caches = &process_cache_bases().unwrap()[0];
    assert!(
        !profile.contains(&socket_deny(stage_caches)),
        "the cache bases are named only when provisioning: {profile}"
    );
}
