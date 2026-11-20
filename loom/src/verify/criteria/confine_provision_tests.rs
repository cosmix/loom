//! Provisioning's Seatbelt network rules: the network stays reachable, Unix
//! sockets in every stage cache root and the session scratch base do not,
//! and the host-job socket's later grant still applies.

use super::*;

#[test]
fn a_regex_prefix_escapes_metacharacters_and_refuses_a_quote() {
    let prefix = regex_prefix(Path::new("/a.b/c+d(e)/")).unwrap();
    assert_eq!(prefix, r"^/a\.b/c\+d\(e\)/");
    assert!(regex_prefix(Path::new("/a\"b")).is_err());
}

#[test]
fn provisioning_denies_the_stage_roots_and_the_scratch_base() {
    let root = Path::new("/cache/loom/stage-cache/0123");
    let grants = Grants {
        git: None,
        cache_root: Some(root),
    };
    let rules = provision_network_rules(&grants).unwrap();
    assert!(rules.starts_with("(allow network-outbound)"), "{rules}");
    let denied =
        r#"(deny network-outbound (remote unix-socket (path-regex #"^/cache/loom/stage-cache/")))"#;
    assert!(rules.contains(denied), "{rules}");
    let scratch = crate::relay::scratch_root_from_env().unwrap();
    let scratch = regex_prefix(&scratch).unwrap();
    assert!(rules.contains(&format!("#\"{scratch}\"")), "{rules}");
}

/// `sandbox-exec` parses the rules with the host-job grant after them. Exit
/// 65 is a profile parse error; this session's sandbox may refuse to nest
/// another (71), which still means the profile parsed.
#[test]
fn the_provision_rules_parse() {
    let root = Path::new("/tmp/loom-stage-cache/0123");
    let grants = Grants {
        git: None,
        cache_root: Some(root),
    };
    let mut profile = String::from("(version 1)(allow default)");
    profile.push_str(&provision_network_rules(&grants).unwrap());
    profile.push_str(&sccache_socket_rule(root).unwrap());
    let status = Command::new("/usr/bin/sandbox-exec")
        .arg("-p")
        .arg(&profile)
        .arg("/usr/bin/true")
        .status()
        .unwrap();
    assert_ne!(status.code(), Some(65), "{profile}");
}

/// Provisioning fetches: the prepared command carries the session's `GOPROXY`
/// chain (seed, then the host's or the default proxy), never a host job's
/// `,off` tail. The command is inspected, not run.
#[test]
#[serial_test::serial]
fn provisioning_takes_the_session_goproxy_chain() {
    use crate::stage_cache::{go_proxy, host_env_from_process, Consumer, StageCache};
    if crate::process::sandbox_probe::skip_unless(
        super::super::launcher().is_some(),
        "provisioning_takes_the_session_goproxy_chain",
        "OS sandbox launcher absent",
    ) {
        return;
    }
    // A host job's own GOPROXY ends in `,off`, and the session chain inherits it.
    let host_job = std::env::var("GOPROXY").is_ok_and(|value| value.ends_with(",off"));
    if crate::process::sandbox_probe::skip_unless(
        !host_job,
        "provisioning_takes_the_session_goproxy_chain",
        "the process GOPROXY is a host job's",
    ) {
        return;
    }
    let dirs = ["repo", "home", "tree"].map(|name| {
        let dir = tempfile::tempdir().unwrap();
        (dir.path().canonicalize().unwrap(), dir, name)
    });
    let [(repo, _r, _), (home, _h, _), (tree, _t, _)] = dirs;
    let cache = StageCache::resolve(&repo, "alpha", &home, None).unwrap();
    cache.ensure().unwrap();
    let mut original = Command::new("/bin/sh");
    original.current_dir(&tree);
    let network = super::super::NetworkAccess::Provision;
    let (wrapped, _scratch, _git) =
        super::super::wrap(&original, Some(&tree), network, Some(&cache)).unwrap();
    let proxy = wrapped
        .get_envs()
        .find(|(name, _)| *name == "GOPROXY")
        .and_then(|(_, value)| value)
        .expect("provisioning sets GOPROXY")
        .to_os_string();
    let host = host_env_from_process();
    assert_eq!(proxy, go_proxy(Consumer::Session, &host));
    assert_ne!(proxy, go_proxy(Consumer::HostJob, &host));
    assert!(!proxy.to_string_lossy().ends_with(",off"), "{proxy:?}");
}
