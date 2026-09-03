use super::*;
use crate::models::stage::{
    CommandConfinement, FilesystemConfig, Implementer, Implementers, LinuxConfig, NetworkConfig,
    PermissionMode,
};

fn config() -> MergedSandboxConfig {
    MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: Vec::new(),
        filesystem: FilesystemConfig {
            deny_read: vec!["~/.ssh/**".to_string(), "../../**".to_string()],
            deny_write: Vec::new(),
            allow_write: Vec::new(),
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    }
}

#[test]
fn adds_mandatory_credentials_and_filters_parent_traversal() {
    let paths = deny_read_patterns(&config());
    assert_eq!(paths, vec!["~/.ssh/**", "~/.claude/.credentials.json"]);
}

#[test]
fn grants_the_codex_lane_its_state_dirs() {
    // Without these the first forward dies on `Read-only file system` /
    // `ENOENT: ... mkdir` before any model call, and the escape hatch the
    // agent reaches for next is refused by the auto-mode classifier.
    let mut config = config();
    config.implementers = Implementers::new(vec![Implementer::Codex]);
    let sandbox = sandbox_settings(&config);
    let allow_write: Vec<&str> = sandbox["filesystem"]["allowWrite"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for expected in CODEX_SANDBOX_WRITE_PATHS {
        assert!(
            allow_write.contains(&expected),
            "missing codex write path {expected}"
        );
    }
    // Package-manager caches are granted to every stage; codex's own
    // state dirs come after them, matching filesystem_settings' order.
    let last_package_cache_index = allow_write
        .iter()
        .rposition(|path| PACKAGE_MANAGER_CACHE_WRITE_PATHS.contains(path))
        .expect("package caches must be present");
    let first_codex_index = allow_write
        .iter()
        .position(|path| CODEX_SANDBOX_WRITE_PATHS.contains(path))
        .expect("codex paths must be present");
    assert!(
        first_codex_index > last_package_cache_index,
        "codex paths must come after package-manager caches, got: {allow_write:?}"
    );
    let domains = sandbox["network"]["allowedDomains"].as_array().unwrap();
    for expected in CODEX_SANDBOX_DOMAINS {
        assert!(
            domains.iter().any(|domain| domain == expected),
            "missing codex domain {expected}"
        );
    }
}

#[test]
fn keeps_plan_domains_and_does_not_duplicate_codex_ones() {
    let mut config = config();
    config.implementers = Implementers::new(vec![Implementer::Codex]);
    config.network.allowed_domains = vec!["crates.io".to_string(), "chatgpt.com".to_string()];
    let network = network_settings(&config).unwrap();
    let domains: Vec<&str> = network["allowedDomains"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(domains.contains(&"crates.io"));
    assert_eq!(
        domains.iter().filter(|d| **d == "chatgpt.com").count(),
        1,
        "a plan that already lists a codex domain must not get it twice"
    );
}

#[test]
fn emits_explicit_false_sandbox_booleans() {
    let mut config = config();
    config.auto_allow = false;
    config.allow_unsandboxed_escape = false;

    let sandbox = sandbox_settings(&config);

    assert_eq!(sandbox["autoAllowBashIfSandboxed"], json!(false));
    assert_eq!(sandbox["allowUnsandboxedCommands"], json!(false));
}

#[test]
fn enabled_claude_only_network_is_strict_without_domains() {
    let mut config = config();
    config.network.allowed_domains = Vec::new();
    config.network.additional_domains = Vec::new();

    let network = network_settings(&config).unwrap();

    assert_eq!(network["strictAllowlist"], json!(true));
    assert!(
        network["allowedDomains"].is_null()
            || network["allowedDomains"]
                .as_array()
                .is_some_and(|domains| domains.is_empty())
    );
}

#[test]
fn claude_only_stages_do_not_receive_codex_domains() {
    let mut config = config();
    config.network.allowed_domains = Vec::new();
    config.network.additional_domains = Vec::new();

    let network = network_settings(&config).unwrap();
    let domains = network["allowedDomains"].as_array();
    for expected in CODEX_SANDBOX_DOMAINS {
        assert!(
            domains
                .map(|domains| !domains.iter().any(|domain| domain == expected))
                .unwrap_or(true),
            "unexpected codex domain {expected}"
        );
    }
}

#[test]
fn codex_licensed_stages_receive_codex_domains() {
    let mut config = config();
    config.implementers = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);
    config.network.allowed_domains = Vec::new();
    config.network.additional_domains = Vec::new();

    let network = network_settings(&config).unwrap();
    let domains = network["allowedDomains"].as_array().unwrap();
    for expected in CODEX_SANDBOX_DOMAINS {
        assert!(
            domains.iter().any(|domain| domain == expected),
            "missing codex domain {expected}"
        );
    }
}

#[test]
fn claude_only_allow_write_is_plan_paths_then_package_caches() {
    let mut config = config();
    config.filesystem.allow_write = vec!["src/**".to_string()];

    let filesystem = filesystem_settings(&config).unwrap();
    let allow_write: Vec<&str> = filesystem["allowWrite"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();

    assert_eq!(allow_write.first(), Some(&"src/**"));
    for expected in CODEX_SANDBOX_WRITE_PATHS {
        assert!(
            !allow_write.contains(&expected),
            "claude-only stage must not receive codex path {expected}"
        );
    }
    for expected in PACKAGE_MANAGER_CACHE_WRITE_PATHS {
        assert!(
            allow_write.contains(&expected),
            "missing package-manager cache {expected}"
        );
    }
}

#[test]
fn codex_licensed_allow_write_appends_codex_state_paths() {
    let mut config = config();
    config.implementers = Implementers::new(vec![Implementer::Codex, Implementer::Claude]);
    config.filesystem.allow_write = vec!["src/**".to_string()];

    let filesystem = filesystem_settings(&config).unwrap();

    let mut expected: Vec<&str> = vec!["src/**"];
    expected.extend(PACKAGE_MANAGER_CACHE_WRITE_PATHS);
    expected.extend(CODEX_SANDBOX_WRITE_PATHS);
    assert_eq!(filesystem["allowWrite"], json!(expected));
}

#[test]
fn every_stage_gets_the_package_caches_even_with_no_plan_entries() {
    let config = config();
    assert!(config.filesystem.allow_write.is_empty());
    assert!(!config.implementers.includes_codex());

    let filesystem = filesystem_settings(&config).unwrap();

    assert_eq!(
        filesystem["allowWrite"],
        json!(PACKAGE_MANAGER_CACHE_WRITE_PATHS)
    );
}

#[test]
fn rejects_every_command_exclusion() {
    let mut config = config();
    config.excluded_commands = vec!["git:*".to_string()];
    let error = validate_emittable(&config).unwrap_err().to_string();
    assert!(error.contains("excluded_commands"));
    assert!(error.contains("outside the host sandbox"));
}
