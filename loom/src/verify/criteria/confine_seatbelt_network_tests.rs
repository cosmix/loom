//! The network rules of the Seatbelt profile, rendered on any host: no IP
//! networking by default, the loopback grant of a stage that sets
//! `allow_local_binding`, and provisioning's Unix-socket denials (a cache root
//! outside `<cache base>/loom/stage-cache` still keeps every stage cache and
//! base-run cache below this process's cache base out of reach).

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::{
    process_cache_bases, profile, regex_prefix, CargoScratch, Grants, NetworkAccess,
    LOOPBACK_NETWORK, SCCACHE_PORT_DENY,
};
use crate::models::stage::{NetworkConfig, Stage};
use crate::stage_cache::StageCache;
use crate::verify::criteria::{plan_local_binding, stage_network};

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

// The cache bases come from the process's HOME and XDG_CACHE_HOME, which a
// confined completion run exports as its stage cache: pin them.
#[test]
#[serial_test::serial]
fn the_process_cache_bases_are_the_stage_cache_and_base_run_parents() {
    let _env = crate::test_cache_env::CacheEnv::pin();
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
#[serial_test::serial]
fn provisioning_with_a_base_cache_denies_every_process_cache_base_before_its_own_socket() {
    let _env = crate::test_cache_env::CacheEnv::pin();
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
#[serial_test::serial]
fn a_denied_network_renders_no_provisioning_rules() {
    let _env = crate::test_cache_env::CacheEnv::pin();
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

#[test]
#[serial_test::serial]
fn a_denied_network_grants_no_ip_networking() {
    let _env = crate::test_cache_env::CacheEnv::pin();
    let profile = render(&layout(), NetworkAccess::Denied);
    for rule in [
        "(local ip",
        "(remote ip",
        "(remote tcp",
        "(allow network-outbound)",
    ] {
        assert!(
            !profile.contains(rule),
            "{rule} in a denied profile: {profile}"
        );
    }
}

#[test]
#[serial_test::serial]
fn a_loopback_network_grants_this_host_only_and_keeps_sccache_unreachable() {
    let _env = crate::test_cache_env::CacheEnv::pin();
    let profile = render(&layout(), NetworkAccess::Loopback);
    let grant = profile.find(LOOPBACK_NETWORK).expect("the loopback grant");
    let deny = profile
        .find(SCCACHE_PORT_DENY)
        .expect("the sccache port deny");
    assert!(
        grant < deny,
        "the later deny wins over the grant: {profile}"
    );
    assert_eq!(profile.matches("(local ip").count(), 1, "{profile}");
    // macOS 26 honours no port deny after a `(remote ip ...)` allow, so the
    // grant names `tcp` and `udp` separately (`LOOPBACK_NETWORK`).
    assert_eq!(profile.matches("(remote ip").count(), 0, "{profile}");
    for protocol in ["tcp", "udp"] {
        let rule = format!("(allow network-outbound (remote {protocol} \"localhost:*\"))");
        assert_eq!(profile.matches(&rule).count(), 1, "{profile}");
    }
    for rule in ["\"*:*\"", "(allow network-outbound)"] {
        assert!(
            !profile.contains(rule),
            "{rule} in a loopback profile: {profile}"
        );
    }
}

#[test]
#[serial_test::serial]
fn provisioning_keeps_its_network_and_gains_no_loopback_rule() {
    let _env = crate::test_cache_env::CacheEnv::pin();
    let profile = render(&layout(), NetworkAccess::Provision);
    assert!(profile.contains("(allow network-outbound)"), "{profile}");
    for rule in [LOOPBACK_NETWORK, SCCACHE_PORT_DENY, "(local ip"] {
        assert!(
            !profile.contains(rule),
            "{rule} in a provision profile: {profile}"
        );
    }
}

fn stage_with(network: Option<bool>) -> Stage {
    let mut stage = Stage::new("loopback".to_string(), None);
    stage.sandbox.network = network.map(|allow_local_binding| NetworkConfig {
        allow_local_binding,
        ..NetworkConfig::default()
    });
    stage
}

/// A stage's own `network` section replaces the plan's whole, as
/// `sandbox::merge_config` merges it, and the rendered profile follows.
#[test]
#[serial_test::serial]
fn a_stages_allow_local_binding_reaches_the_profile_and_is_absent_by_default() {
    let _env = crate::test_cache_env::CacheEnv::pin();
    let layout = layout();
    let cases = [
        (None, false, NetworkAccess::Denied),
        (None, true, NetworkAccess::Loopback),
        (Some(true), false, NetworkAccess::Loopback),
        (Some(false), true, NetworkAccess::Denied),
    ];
    for (stage, plan, expected) in cases {
        let network = stage_network(&stage_with(stage), plan);
        assert!(network == expected, "stage {stage:?} over plan {plan}");
        let granted = render(&layout, network).contains(LOOPBACK_NETWORK);
        assert_eq!(
            granted,
            expected == NetworkAccess::Loopback,
            "{stage:?} {plan}"
        );
    }
}

#[test]
fn the_plan_snapshot_supplies_the_plan_level_local_binding() {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    assert!(
        !plan_local_binding(&work),
        "no snapshot is the fail-safe level"
    );
    let mut sandbox = crate::plan::schema::SandboxConfig::default();
    crate::fs::work_dir::write_plan_sandbox(&work, &sandbox).unwrap();
    assert!(
        !plan_local_binding(&work),
        "the default plan grants nothing"
    );
    sandbox.network.allow_local_binding = true;
    crate::fs::work_dir::write_plan_sandbox(&work, &sandbox).unwrap();
    assert!(plan_local_binding(&work));
}
