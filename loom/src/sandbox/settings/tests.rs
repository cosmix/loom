use super::*;
use crate::models::stage::{Implementer, Implementers};
use crate::plan::schema::{
    CommandConfinement, FilesystemConfig, LinuxConfig, NetworkConfig, SandboxConfig,
    StageSandboxConfig, StageType,
};
use crate::sandbox::{merge_config, PACKAGE_MANAGER_CACHE_WRITE_PATHS};

fn default_config() -> MergedSandboxConfig {
    MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    }
}

/// A stage config that licenses the codex lane - the gate
/// `preserve_unowned_keys` needs before it will carry `enabledPlugins` /
/// `extraKnownMarketplaces` forward from an existing settings file.
fn codex_licensed_config() -> MergedSandboxConfig {
    let mut config = default_config();
    config.implementers = Implementers::new(vec![Implementer::Codex]);
    config
}

/// `allowWrite`: prefix paths then every granted package-manager cache.
fn allow_write_with_caches(prefix: &[&str]) -> Value {
    let mut expected: Vec<&str> = prefix.to_vec();
    expected.extend(PACKAGE_MANAGER_CACHE_WRITE_PATHS);
    json!(expected)
}

#[test]
fn test_apply_default_mode_matrix() {
    // Each PermissionMode → camelCase string emitted into settings JSON.
    let cases = [
        (PermissionMode::Default, "default"),
        (PermissionMode::AcceptEdits, "acceptEdits"),
        (PermissionMode::Auto, "auto"),
        (PermissionMode::Plan, "plan"),
        (PermissionMode::BypassPermissions, "bypassPermissions"),
    ];
    for (mode, expected) in cases {
        let mut settings = json!({});
        apply_default_mode(&mut settings, mode).unwrap();
        assert_eq!(
            settings["permissions"]["defaultMode"],
            json!(expected),
            "mode {mode:?} should serialize to {expected}"
        );
    }
}

#[test]
fn test_apply_default_mode_preserves_existing_permissions() {
    let mut settings = json!({
        "permissions": {
            "allow": ["Read(.work/**)"]
        }
    });
    apply_default_mode(&mut settings, PermissionMode::Plan).unwrap();
    assert_eq!(settings["permissions"]["defaultMode"], json!("plan"));
    assert_eq!(
        settings["permissions"]["allow"],
        json!(["Read(.work/**)"]),
        "Existing allow list must be preserved"
    );
}

#[test]
fn test_permission_mode_kebab_case_round_trip() {
    // Round-trip through serde_yaml using kebab-case spelling.
    for (mode, kebab) in [
        (PermissionMode::Default, "default"),
        (PermissionMode::AcceptEdits, "accept-edits"),
        (PermissionMode::Auto, "auto"),
        (PermissionMode::Plan, "plan"),
        (PermissionMode::BypassPermissions, "bypass-permissions"),
    ] {
        let yaml = serde_yaml::to_string(&mode).unwrap();
        assert!(yaml.contains(kebab), "{mode:?} should serialize to {kebab}");
        let back: PermissionMode = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, mode);
    }
}

#[test]
fn test_generate_settings_json_includes_resolved_default_mode() {
    // Each PermissionMode in MergedSandboxConfig becomes the camelCase
    // permissions.defaultMode in the generated settings JSON.
    for (mode, expected) in [
        (PermissionMode::Default, "default"),
        (PermissionMode::AcceptEdits, "acceptEdits"),
        (PermissionMode::Auto, "auto"),
        (PermissionMode::Plan, "plan"),
        (PermissionMode::BypassPermissions, "bypassPermissions"),
    ] {
        let config = MergedSandboxConfig {
            enabled: true,
            auto_allow: true,
            allow_unsandboxed_escape: false,
            excluded_commands: vec![],
            filesystem: FilesystemConfig::default(),
            network: NetworkConfig::default(),
            linux: LinuxConfig::default(),
            permission_mode: mode,
            implementers: Implementers::default(),
            command_confinement: CommandConfinement::default(),
        };
        let json = generate_settings_json(&config);
        assert_eq!(
            json["permissions"]["defaultMode"],
            json!(expected),
            "generate_settings_json must emit camelCase for {mode:?}"
        );
    }
}

#[test]
fn test_generate_settings_disables_worktree_isolation() {
    // Loom owns the worktree, so Claude Code's bgIsolation must be "none"
    // to keep subagents from spawning nested worktrees/branches.
    let config = default_config();

    let json = generate_settings_json(&config);
    assert_eq!(json["worktree"]["bgIsolation"], json!("none"));
}

#[test]
fn test_generate_settings_disabled() {
    let mut config = default_config();
    config.enabled = false;

    let json = generate_settings_json(&config);
    // Sandbox block should have enabled: false
    assert_eq!(json["sandbox"]["enabled"], false);
}

#[test]
fn test_generate_settings_with_filesystem() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec!["~/.ssh/**".to_string(), "../../**".to_string()],
            deny_write: vec![".loom/work/**".to_string()],
            allow_write: vec!["src/**".to_string()],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    assert_filesystem_sandbox(&json);

    // Permissions for file tool restrictions
    // Parent-traversal deny_read paths (../../**) are filtered out because
    // Claude Code leaks them into the OS sandbox where they resolve too broadly
    let deny = json["permissions"]["deny"].as_array().unwrap();
    assert_eq!(deny.len(), 3);
    assert_eq!(deny[0], "Read(~/.ssh/**)");
    assert_eq!(deny[1], "Read(~/.claude/.credentials.json)");
    assert_eq!(deny[2], "Edit(.loom/work/**)");

    // allow_write paths come first, then the narrowly-scoped state
    // permissions agents need (signals/handoffs/disputes/memory), emitted in
    // both layout spellings: the nested `.loom/work/...` six, then the
    // legacy `.work/...` six. A workspace that resolved to a legacy
    // `<repo>/.work/` root stays legacy forever, and this function cannot
    // see which layout it is emitting for, so it emits both; on either
    // layout the other spelling matches nothing and costs nothing. The set
    // is deliberately scoped to subdirs an agent touches — never bare
    // `.loom/work/**` / `.work/**`, which would also expose
    // `admin.token` / `user.token` (S-1).
    let allow = json["permissions"]["allow"].as_array().unwrap();
    assert_eq!(allow.len(), 13);
    assert_eq!(allow[0], "Edit(src/**)");
    assert_eq!(allow[1], "Read(.loom/work/config.toml)");
    assert_eq!(allow[2], "Read(.loom/work/signals/**)");
    assert_eq!(allow[3], "Read(.loom/work/handoffs/**)");
    assert_eq!(allow[4], "Edit(.loom/work/handoffs/**)");
    assert_eq!(allow[5], "Read(.loom/work/disputes/**)");
    assert_eq!(allow[6], "Read(.loom/work/memory/**)");
    assert_eq!(allow[7], "Read(.work/config.toml)");
    assert_eq!(allow[8], "Read(.work/signals/**)");
    assert_eq!(allow[9], "Read(.work/handoffs/**)");
    assert_eq!(allow[10], "Edit(.work/handoffs/**)");
    assert_eq!(allow[11], "Read(.work/disputes/**)");
    assert_eq!(allow[12], "Read(.work/memory/**)");
}

fn assert_filesystem_sandbox(json: &Value) {
    assert_eq!(json["sandbox"]["enabled"], true);
    assert_eq!(json["sandbox"]["failIfUnavailable"], true);
    assert_eq!(json["sandbox"]["autoAllowBashIfSandboxed"], true);
    let fs_block = &json["sandbox"]["filesystem"];
    let deny_read = fs_block["denyRead"].as_array().unwrap();
    assert!(deny_read.iter().any(|value| value == "~/.ssh/**"));
    assert!(deny_read
        .iter()
        .any(|value| value == "~/.claude/.credentials.json"));
    // allowWrite: plan entries then package-manager caches (claude-only).
    assert_eq!(fs_block["allowWrite"], allow_write_with_caches(&["src/**"]));
    let deny_write = fs_block["denyWrite"].as_array().unwrap();
    assert_eq!(deny_write.len(), 1);
    assert_eq!(deny_write[0], ".loom/work/**");
}

#[test]
fn test_generate_settings_with_network() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig {
            allowed_domains: vec!["*.github.com".to_string()],
            additional_domains: vec!["api.example.com".to_string()],
            allow_local_binding: true,
            allow_unix_sockets: vec!["/tmp/*.sock".to_string()],
            allow_all_unix_sockets: false,
        },
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);

    // Network config is now in sandbox.network block
    let network = &json["sandbox"]["network"];
    let domains = network["allowedDomains"].as_array().unwrap();
    // Claude-only stage: exactly the plan's own domains, no codex lane hosts.
    assert_eq!(domains.len(), 2);
    assert!(domains.iter().any(|d| d == "*.github.com"));
    assert!(domains.iter().any(|d| d == "api.example.com"));
    assert!(
        !domains.iter().any(|d| d == "chatgpt.com"),
        "claude-only stage must not receive codex domains, got: {:?}",
        domains
    );
    assert_eq!(network["allowLocalBinding"], true);
    let sockets = network["allowUnixSockets"].as_array().unwrap();
    assert_eq!(sockets.len(), 1);
    assert_eq!(sockets[0], "/tmp/*.sock");
}

#[test]
fn test_generate_settings_with_linux_config() {
    let mut config = default_config();
    config.linux.enable_weaker_nested = true;

    let json = generate_settings_json(&config);
    assert_eq!(json["sandbox"]["enableWeakerNestedSandbox"], true);
}

#[test]
fn test_generate_settings_never_emits_excluded_commands() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec!["loom".to_string(), "git".to_string()],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    assert!(json["sandbox"]["excludedCommands"].is_null());

    let temp = tempfile::TempDir::new().unwrap();
    let error = write_settings(&config, temp.path())
        .unwrap_err()
        .to_string();
    assert!(error.contains("excluded_commands"));
    assert!(!temp.path().join(".claude/settings.local.json").exists());
}

#[test]
fn test_default_sandbox_has_no_command_exclusions() {
    let config = SandboxConfig::default();
    assert!(config.excluded_commands.is_empty());
}

#[test]
fn generated_stage_settings_deny_unix_sockets_for_completion_broker_integrity() {
    let plan = SandboxConfig::default();
    let stage = StageSandboxConfig::default();
    let config = merge_config(&plan, &stage, StageType::Standard, &Implementers::default());

    let json = generate_settings_json(&config);
    let network = &json["sandbox"]["network"];
    assert!(network["allowUnixSockets"].is_null());
    assert!(network["allowAllUnixSockets"].is_null());
}

#[test]
fn test_generate_settings_with_unsandboxed_escape() {
    let mut config = default_config();
    config.allow_unsandboxed_escape = true;

    let json = generate_settings_json(&config);
    // allowUnsandboxedCommands is now in sandbox block
    assert_eq!(json["sandbox"]["allowUnsandboxedCommands"], true);
}

#[test]
fn test_generate_settings_exclusions_never_get_bash_allow() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec!["loom".to_string(), "git".to_string()],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    let allow = json["permissions"]["allow"].as_array().unwrap();

    let allow_strs: Vec<&str> = allow.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(!allow_strs
        .iter()
        .any(|entry| entry.starts_with("Bash(loom")));
    assert!(!allow_strs.iter().any(|entry| entry.starts_with("Bash(git")));
}

#[test]
fn test_generate_settings_includes_work_dir_read_allows() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    let allow = json["permissions"]["allow"].as_array().unwrap();

    let allow_strs: Vec<&str> = allow.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(
        allow_strs.contains(&"Read(.loom/work/signals/**)"),
        "Should allow reading signals, got: {:?}",
        allow_strs
    );
    assert!(
        allow_strs.contains(&"Read(.loom/work/handoffs/**)"),
        "Should allow reading handoffs, got: {:?}",
        allow_strs
    );
    assert!(
        allow_strs.contains(&"Read(.loom/work/config.toml)"),
        "Should allow reading config, got: {:?}",
        allow_strs
    );
}

#[test]
fn test_generate_settings_keeps_mandatory_credential_deny_when_empty() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![],
            deny_write: vec![],
            allow_write: vec![],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    assert_eq!(
        json["sandbox"]["filesystem"]["denyRead"],
        json!(["~/.claude/.credentials.json"])
    );
}

#[test]
fn test_generate_settings_with_all_unix_sockets() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig {
            allowed_domains: vec![],
            additional_domains: vec![],
            allow_local_binding: false,
            allow_unix_sockets: vec![],
            allow_all_unix_sockets: true,
        },
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    assert_eq!(json["sandbox"]["network"]["allowAllUnixSockets"], true);
}

#[test]
fn test_deny_read_is_enforced_in_os_sandbox() {
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![
                "~/.ssh/**".to_string(),
                "../../**".to_string(),
                "../.worktrees/**".to_string(),
            ],
            deny_write: vec![],
            allow_write: vec![],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);

    let os_deny = json["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    let os_deny_strs: Vec<&str> = os_deny.iter().filter_map(|v| v.as_str()).collect();
    assert!(os_deny_strs.contains(&"~/.ssh/**"));
    assert!(os_deny_strs.contains(&"~/.claude/.credentials.json"));
    assert!(!os_deny_strs.contains(&"../../**"));
    assert!(!os_deny_strs.contains(&"../.worktrees/**"));

    // permissions.deny should have non-traversal paths only
    let deny = json["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
    // Parent-traversal paths must NOT be in permissions.deny because Claude Code
    // leaks them into the OS sandbox where they resolve too broadly
    assert!(
        !deny_strs.contains(&"Read(../../**)"),
        "../../** must NOT be in permissions.deny Read() (leaks into OS sandbox)"
    );
    assert!(
        !deny_strs.contains(&"Read(../.worktrees/**)"),
        "../.worktrees/** must NOT be in permissions.deny Read() (leaks into OS sandbox)"
    );
}

#[test]
fn test_deny_write_parent_traversal_not_in_os_sandbox() {
    // Parent-traversal paths (../) in deny_write must NOT appear in
    // sandbox.filesystem.denyWrite OR in permissions.deny Edit(...).
    // Both layers resolve `../` relative to the project root, causing
    // overly broad restrictions:
    // - From worktrees: "../../**" resolves to an ancestor and, because
    //   `**` crosses separators, matches the worktree's OWN files —
    //   an enforceable `Edit(../../**)` deny would refuse the agent's
    //   first edit to its own source tree.
    // - From the main repo root: "../../**" is $HOME.
    // Worktree write-escape is enforced independently (OS sandbox
    // allowOnly list + worktree hooks), so dropping the tool-layer rule
    // loses no protection. An ordinary non-traversal path DOES still need
    // to reach permissions.deny to be enforceable at all; the knowledge
    // directory is the one deliberate exception (see
    // `every_stage_type_can_write_the_knowledge_directory`) — it is
    // filtered here too, defense-in-depth against a hand-built config
    // that bypassed `merge_config`'s `apply_knowledge_write_grant`.
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![],
            deny_write: vec![
                "../../**".to_string(),
                "some/plan/path/**".to_string(),
                "doc/loom/knowledge/**".to_string(),
            ],
            allow_write: vec![],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);

    // OS sandbox denyWrite must NOT contain parent-traversal or
    // knowledge-dir paths (they resolve too broadly in sandbox-exec, and
    // block the knowledge CLI, respectively); the ordinary non-traversal
    // path DOES reach it, same as it reaches permissions.deny below.
    assert_eq!(
        json["sandbox"]["filesystem"]["denyWrite"],
        json!(["some/plan/path/**"])
    );
    let fs_block = &json["sandbox"]["filesystem"];
    assert_eq!(fs_block["allowWrite"], allow_write_with_caches(&[]));

    // permissions.deny should have the ordinary non-traversal path only;
    // the parent-traversal entry must be filtered, or it would deny-match
    // the worktree's own files once Claude Code enforces `Edit(...)`
    // rules, and the knowledge-dir entry must be filtered so it can never
    // block the `loom knowledge update` CLI subprocess.
    let deny = json["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        !deny_strs.contains(&"Edit(../../**)"),
        "Parent-traversal must NOT be in permissions.deny \
         (matches the worktree's own files, deny wins over allow)"
    );
    assert!(
        deny_strs.contains(&"Edit(some/plan/path/**)"),
        "an ordinary project-relative deny_write entry should still reach permissions.deny"
    );
    assert!(
        !deny_strs.contains(&"Edit(doc/loom/knowledge/**)"),
        "the knowledge directory must never reach permissions.deny, got: {deny_strs:?}"
    );
}

#[test]
fn test_generate_settings_plan_allow_write_reaches_os_sandbox_claude_only() {
    // This is the end-to-end proof for the previously-inert lever: a plan's
    // `filesystem.allow_write` now reaches `sandbox.filesystem.allowWrite`
    // (the OS-enforced grant), even for a claude-only stage that never
    // licenses the codex lane. See doc/loom/knowledge/mistakes/sandbox-and-settings.md
    // § "A Plan's `allow_write` Cannot Grant a Subprocess OS-Level Write Access".
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![],
            deny_write: vec![],
            allow_write: vec!["tmp/tmux-sockets/**".to_string()],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(), // claude-only
        command_confinement: CommandConfinement::default(),
    };
    assert!(!config.implementers.includes_codex());

    let json = generate_settings_json(&config);

    assert_eq!(
        json["sandbox"]["filesystem"]["allowWrite"],
        allow_write_with_caches(&["tmp/tmux-sockets/**"]),
        "must include package caches, no codex paths, got: {json:?}"
    );
}

#[test]
fn test_allow_write_parent_traversal_filtered_but_normal_entry_kept() {
    // A `../` allow_write entry must never reach permissions.allow as an
    // `Edit(...)` rule - it merges into the OS-enforced allowWrite grant
    // and would open write outside the worktree. The sibling ordinary
    // entry proves the loop still works, not just drops everything.
    let config = MergedSandboxConfig {
        filesystem: FilesystemConfig {
            allow_write: vec!["../../escape/**".to_string(), "loom/src/**".to_string()],
            ..Default::default()
        },
        ..default_config()
    };

    let json = generate_settings_json(&config);
    let allow = json["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !allow_strs.iter().any(|rule| rule.contains("../")),
        "parent-traversal allow_write must not reach permissions.allow, got: {allow_strs:?}"
    );
    assert_eq!(allow[0], "Edit(loom/src/**)");
}

#[test]
fn test_allow_write_trims_whitespace_and_drops_empty() {
    let config = MergedSandboxConfig {
        filesystem: FilesystemConfig {
            allow_write: vec!["  loom/src/**  ".to_string(), "   ".to_string()],
            ..Default::default()
        },
        ..default_config()
    };

    let json = generate_settings_json(&config);
    let allow = json["permissions"]["allow"].as_array().unwrap();

    // The padded entry is trimmed and emitted; the whitespace-only entry
    // contributes nothing - allow.len() is 1 (allow_write) + 6 (.loom/work/
    // state permissions) + 6 (legacy .work/ state permissions), same as a
    // single ordinary entry would produce.
    assert_eq!(allow.len(), 13, "got: {allow:?}");
    assert_eq!(allow[0], "Edit(loom/src/**)");
}

#[test]
fn test_generate_settings_emits_network_block() {
    // The native backend emits the sandbox.network block whenever the
    // sandbox is enabled (strictAllowlist), regardless of domain content.
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![],
            deny_write: vec!["some/plan/path/**".to_string()],
            allow_write: vec![],
        },
        network: NetworkConfig {
            allowed_domains: vec!["github.com".to_string(), "api.github.com".to_string()],
            additional_domains: vec![],
            allow_local_binding: true,
            allow_unix_sockets: vec![],
            allow_all_unix_sockets: false,
        },
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let json = generate_settings_json(&config);
    let network = &json["sandbox"]["network"];
    assert!(
        !network.is_null(),
        "sandbox.network must be emitted when allowed_domains is set"
    );
    let domains = network["allowedDomains"]
        .as_array()
        .expect("allowedDomains must be present");
    // Claude-only stage: exactly the plan's own domains, no codex lane hosts.
    assert_eq!(domains.len(), 2);
    assert!(domains.iter().any(|d| d == "github.com"));
    assert!(
        !domains.iter().any(|d| d == "api.openai.com"),
        "claude-only stage must not receive codex domains, got: {:?}",
        domains
    );
    assert_eq!(network["allowLocalBinding"], true);
    assert_eq!(network["strictAllowlist"], json!(true));

    // Filesystem deny entries are emitted alongside the network block.
    let deny = json["permissions"]["deny"]
        .as_array()
        .expect("filesystem deny entries should be present");
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    assert!(deny_strs.contains(&"Edit(some/plan/path/**)"));
}

#[test]
fn test_no_path_in_both_allow_and_deny() {
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
    use crate::sandbox::merge_config;

    // Test all stage types
    for stage_type in [
        StageType::Standard,
        StageType::Knowledge,
        StageType::IntegrationVerify,
        StageType::KnowledgeDistill,
    ] {
        let plan = SandboxConfig::default();
        let stage = StageSandboxConfig::default();
        let merged = merge_config(&plan, &stage, stage_type, &Implementers::default());
        let json = generate_settings_json(&merged);

        let permissions = &json["permissions"];
        if permissions.is_null() {
            continue;
        }

        let allow = permissions["allow"]
            .as_array()
            .map(|a| a.to_vec())
            .unwrap_or_default();
        let deny = permissions["deny"]
            .as_array()
            .map(|a| a.to_vec())
            .unwrap_or_default();

        // Compare full permission strings (e.g. "Read(.loom/work/signals/**)")
        // to detect true conflicts where the same permission type + path
        // appears in both allow and deny.
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

        for entry in &allow_strs {
            assert!(
                !deny_strs.contains(entry),
                "Stage type {:?}: '{}' appears in both allow and deny",
                stage_type,
                entry
            );
        }
    }
}

#[test]
fn every_stage_type_can_write_the_knowledge_directory() {
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
    use crate::sandbox::merge_config;

    // Every stage type must come out of `merge_config` with the
    // knowledge directory GRANTED, not denied: `loom knowledge update`
    // is a Bash subprocess that runs inside the sandbox for every stage
    // type, with no "excluded command" escape hatch to fall back on.
    for stage_type in [
        StageType::Standard,
        StageType::Knowledge,
        StageType::KnowledgeDistill,
        StageType::IntegrationVerify,
    ] {
        let plan = SandboxConfig::default();
        let stage = StageSandboxConfig::default();
        let merged = merge_config(&plan, &stage, stage_type, &Implementers::default());
        let json = generate_settings_json(&merged);

        assert_eq!(
            json["sandbox"]["filesystem"]["allowWrite"],
            allow_write_with_caches(&["doc/loom/knowledge/**"]),
            "Stage type {stage_type:?}: must include package caches, got: {:?}",
            json["sandbox"]["filesystem"]["allowWrite"]
        );

        let allow = json["permissions"]["allow"].as_array().unwrap();
        let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
        assert!(
            allow_strs.contains(&"Edit(doc/loom/knowledge/**)"),
            "Stage type {stage_type:?}: permissions.allow must contain \
             Edit(doc/loom/knowledge/**), got: {allow_strs:?}"
        );

        let deny_strs: Vec<&str> = json["permissions"]["deny"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        assert!(
            !deny_strs.iter().any(|p| p.contains("doc/loom/knowledge")),
            "Stage type {stage_type:?}: permissions.deny must not mention the knowledge \
             directory, got: {deny_strs:?}"
        );
        let os_deny_write = json["sandbox"]["filesystem"]["denyWrite"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
            .unwrap_or_default();
        assert!(
            !os_deny_write
                .iter()
                .any(|p| p.contains("doc/loom/knowledge")),
            "Stage type {stage_type:?}: sandbox.filesystem.denyWrite must not mention the \
             knowledge directory, got: {os_deny_write:?}"
        );
    }
}

#[test]
fn plan_authored_knowledge_deny_write_is_dropped_and_grant_added() {
    use crate::plan::schema::{SandboxConfig, StageSandboxConfig, StageType};
    use crate::sandbox::merge_config;

    // Plans authored before this fix carry `doc/loom/knowledge/**` in
    // their own `filesystem.deny_write` (it used to be the default).
    // `merge_config` must strip that authored entry and add the grant in
    // its place, not just leave the contradiction for the emitter to
    // paper over.
    let plan = SandboxConfig {
        filesystem: FilesystemConfig {
            deny_write: vec!["doc/loom/knowledge/**".to_string()],
            ..FilesystemConfig::default()
        },
        ..SandboxConfig::default()
    };
    let stage = StageSandboxConfig::default();
    let merged = merge_config(&plan, &stage, StageType::Standard, &Implementers::default());

    assert!(
        !merged
            .filesystem
            .deny_write
            .iter()
            .any(|p| p.starts_with("doc/loom/knowledge")),
        "the plan-authored deny_write entry must be dropped, got: {:?}",
        merged.filesystem.deny_write
    );
    assert!(
        merged
            .filesystem
            .allow_write
            .contains(&"doc/loom/knowledge/**".to_string()),
        "the grant must be added in its place, got: {:?}",
        merged.filesystem.allow_write
    );
}

#[test]
fn test_write_settings_preserves_existing_deny_but_not_allow() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path();

    // Create existing settings.local.json with permissions from a prior session.
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.local.json");

    let existing_settings = json!({
        "permissions": {
            "allow": [
                "Read(~/.ssh/config)",
                "Bash(docker:*)"
            ],
            "deny": [
                "Write(~/.bashrc)",
                "Write(**)"
            ]
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&existing_settings).unwrap(),
    )
    .unwrap();

    // Now call write_settings with sandbox config
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![],
            deny_write: vec![],
            allow_write: vec!["src/**".to_string()],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, worktree_path).unwrap();

    // Read the result
    let result_content = fs::read_to_string(&settings_path).unwrap();
    let result: Value = serde_json::from_str(&result_content).unwrap();

    // Verify sandbox-generated permissions are present
    let allow = result["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
    assert!(allow_strs.contains(&"Edit(src/**)"));
    assert!(allow_strs.contains(&"Read(.loom/work/signals/**)"));

    // SECURITY: existing `allow` entries are NOT carried forward. Allow is
    // regenerated purely from config on every write - that is what stops a
    // stage agent from self-granting a persistent permission by writing it
    // into its own (agent-writable, respawn-reused) settings.local.json.
    assert!(!allow_strs.contains(&"Read(~/.ssh/config)"));
    assert!(!allow_strs.contains(&"Bash(docker:*)"));

    // `deny` is still merged forward - widening deny can only narrow what
    // the agent can do, never grant it anything, so carrying it forward
    // is safe (unlike `allow`). It is carried forward in the ENFORCEABLE
    // spelling: Claude Code's file permission check consults only
    // `Edit(path)`, so re-emitting the user's `Write(~/.bashrc)` verbatim
    // would preserve a rule that blocks nothing and warns at every session
    // start. The intent survives, the inert form does not.
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    assert!(deny_strs.contains(&"Edit(~/.bashrc)"), "got: {deny_strs:?}");
    assert!(
        !deny_strs.contains(&"Write(~/.bashrc)"),
        "got: {deny_strs:?}"
    );

    // A blanket carried deny is dropped outright rather than migrated:
    // enforced as `Edit(**)` it would deny the agent's every edit, and as
    // `Write(**)` it denies nothing - there is no form worth keeping.
    assert!(
        !deny_strs.contains(&"Write(**)") && !deny_strs.contains(&"Edit(**)"),
        "a blanket carried deny must not survive in either form, got: {deny_strs:?}"
    );
}

#[test]
fn test_write_settings_does_not_carry_forward_existing_allow() {
    use tempfile::TempDir;

    // SECURITY regression test: `.claude/settings.local.json` lives inside
    // the agent-writable worktree, and worktrees are REUSED across respawn
    // / retry / crash recovery (`orchestrator/core/stage_executor.rs` via
    // `git::get_or_create_worktree`). If `permissions.allow` were merged
    // forward the way `deny` is, a stage agent could append an entry to its
    // own file and have it survive into its next session - including an
    // entry that widens what it can write.
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path();

    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.local.json");

    let existing_settings = json!({
        "permissions": {
            "allow": [
                "Read(.loom/work/signals/**)",  // overlaps a generated entry
                "Read(custom/path/**)",    // an innocuous-looking extra grant
                "Edit(../../**)"           // the escalation this test guards against
            ]
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&existing_settings).unwrap(),
    )
    .unwrap();

    // Call write_settings
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, worktree_path).unwrap();

    // Read the result
    let result_content = fs::read_to_string(&settings_path).unwrap();
    let result: Value = serde_json::from_str(&result_content).unwrap();

    let allow = result["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<String> = allow
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // The generated entry appears exactly once - regeneration never
    // duplicates its own output.
    let signal_count = allow_strs
        .iter()
        .filter(|s| *s == "Read(.loom/work/signals/**)")
        .count();
    assert_eq!(
        signal_count, 1,
        "Read(.loom/work/signals/**) should appear exactly once"
    );

    // Neither an innocuous-looking nor an escalating existing allow entry
    // survives regeneration.
    assert!(
        !allow_strs.contains(&"Read(custom/path/**)".to_string()),
        "existing allow entries must not be carried forward, got: {:?}",
        allow_strs
    );
    assert!(
        !allow_strs.contains(&"Edit(../../**)".to_string()),
        "a self-granted escalating allow entry must not survive, got: {:?}",
        allow_strs
    );
}

#[test]
fn test_write_settings_no_existing_file() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path();

    // Call write_settings with no existing file
    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec!["~/.ssh/**".to_string(), "../../**".to_string()],
            deny_write: vec![],
            allow_write: vec!["src/**".to_string()],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, worktree_path).unwrap();

    // Read the result
    let settings_path = worktree_path.join(".claude/settings.local.json");
    let result_content = fs::read_to_string(&settings_path).unwrap();
    let result: Value = serde_json::from_str(&result_content).unwrap();

    // Verify expected permissions (same as before, no existing to merge)
    let allow = result["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();
    assert!(allow_strs.contains(&"Edit(src/**)"));
    assert!(allow_strs.contains(&"Read(.loom/work/signals/**)"));

    // permissions.deny includes non-traversal deny_read paths
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
    // Parent-traversal paths filtered out (leaked into OS sandbox otherwise)
    assert!(!deny_strs.contains(&"Read(../../**)"));

    let os_deny = result["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    assert!(os_deny.iter().any(|value| value == "~/.ssh/**"));
    assert!(os_deny
        .iter()
        .any(|value| value == "~/.claude/.credentials.json"));
}

#[cfg(unix)]
#[test]
fn test_write_settings_adds_resolved_work_symlink_permissions_legacy_layout() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Simulate a legacy layout: repo_root/.work and repo_root/.worktrees/stage/
    let work_dir = base.join(".work");
    fs::create_dir_all(&work_dir).unwrap();
    fs::create_dir_all(work_dir.join("signals")).unwrap();

    let worktree_path = base.join(".worktrees").join("my-stage");
    fs::create_dir_all(&worktree_path).unwrap();

    // Create the symlink: .worktrees/my-stage/.work -> ../../.work
    std::os::unix::fs::symlink("../../.work", worktree_path.join(".work")).unwrap();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, &worktree_path).unwrap();

    let settings_path = worktree_path.join(".claude/settings.local.json");
    let result_content = fs::read_to_string(&settings_path).unwrap();
    let result: Value = serde_json::from_str(&result_content).unwrap();

    let allow = result["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();

    let resolved_work = work_dir.canonicalize().unwrap();
    let resolved_str = resolved_work.to_string_lossy();

    // S-1: the broad `**` allow over the resolved .work root is GONE — it
    // would have exposed the daemon tokens. Narrowed subdir grants remain.
    let broad_read = format!("Read(/{}/**)", resolved_str);
    let broad_edit = format!("Edit(/{}/**)", resolved_str);
    assert!(
        !allow_strs.contains(&broad_read.as_str()),
        "broad Read(/.work/**) must NOT be granted, got: {:?}",
        allow_strs
    );
    assert!(
        !allow_strs.contains(&broad_edit.as_str()),
        "broad Edit(/.work/**) must NOT be granted, got: {:?}",
        allow_strs
    );

    // Narrowed resolved-absolute grants for the subdirs agents need
    // (signals/ supplies reads; handoffs/ supplies the EROFS write exemption).
    // Emitted as `Edit(...)`, not `Write(...)` — Claude Code's file
    // permission check only consults `Edit(path)` rules.
    let expected_read_signals = format!("Read(/{}/signals/**)", resolved_str);
    let expected_edit_handoffs = format!("Edit(/{}/handoffs/**)", resolved_str);
    assert!(
        allow_strs.contains(&expected_read_signals.as_str()),
        "Should have resolved .work/signals read permission, got: {:?}",
        allow_strs
    );
    assert!(
        allow_strs.contains(&expected_edit_handoffs.as_str()),
        "Should have resolved .work/handoffs edit permission (EROFS exemption), got: {:?}",
        allow_strs
    );

    // S-1: the daemon tokens must be explicitly denied in resolved-absolute form.
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    let deny_admin = format!("Read(/{}/admin.token)", resolved_str);
    let deny_user = format!("Read(/{}/user.token)", resolved_str);
    assert!(
        deny_strs.contains(&deny_admin.as_str()),
        "admin.token must be denied (resolved-absolute), got: {:?}",
        deny_strs
    );
    assert!(
        deny_strs.contains(&deny_user.as_str()),
        "user.token must be denied (resolved-absolute), got: {:?}",
        deny_strs
    );
    let os_deny = result["sandbox"]["filesystem"]["denyRead"]
        .as_array()
        .unwrap();
    let os_admin = format!("/{resolved_str}/admin.token");
    let os_user = format!("/{resolved_str}/user.token");
    assert!(os_deny.iter().any(|value| value == &os_admin));
    assert!(os_deny.iter().any(|value| value == &os_user));

    // Should also still have the relative permissions
    assert!(allow_strs.contains(&"Read(.loom/work/signals/**)"));
    // The legacy spelling is emitted alongside the nested one (this fixture
    // IS a legacy `.work` layout, so this is the rule that actually matches
    // here) — `generate_settings_json` can't see which layout it's on, so it
    // emits both.
    assert!(allow_strs.contains(&"Read(.work/signals/**)"));
}

#[cfg(unix)]
#[test]
fn test_write_settings_adds_resolved_work_symlink_permissions_nested_layout() {
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    // Simulate the nested layout: repo_root/.loom/work and
    // repo_root/.worktrees/stage/.loom/work (a real .loom/ holding the link).
    let work_dir = base.join(".loom").join("work");
    fs::create_dir_all(&work_dir).unwrap();
    fs::create_dir_all(work_dir.join("signals")).unwrap();

    let worktree_path = base.join(".worktrees").join("my-stage");
    let worktree_loom = worktree_path.join(".loom");
    fs::create_dir_all(&worktree_loom).unwrap();

    // Create the symlink: .worktrees/my-stage/.loom/work -> ../../../.loom/work
    std::os::unix::fs::symlink("../../../.loom/work", worktree_loom.join("work")).unwrap();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, &worktree_path).unwrap();

    let settings_path = worktree_path.join(".claude/settings.local.json");
    let result_content = fs::read_to_string(&settings_path).unwrap();
    let result: Value = serde_json::from_str(&result_content).unwrap();

    let allow = result["permissions"]["allow"].as_array().unwrap();
    let allow_strs: Vec<&str> = allow.iter().filter_map(|v| v.as_str()).collect();

    let resolved_work = work_dir.canonicalize().unwrap();
    let resolved_str = resolved_work.to_string_lossy();

    // The nested `.loom/work` link must be the one resolved: same narrow
    // grants as the legacy arm, no broad `**` allow (S-1).
    let broad_read = format!("Read(/{}/**)", resolved_str);
    let broad_edit = format!("Edit(/{}/**)", resolved_str);
    assert!(!allow_strs.contains(&broad_read.as_str()));
    assert!(!allow_strs.contains(&broad_edit.as_str()));

    let expected_read_signals = format!("Read(/{}/signals/**)", resolved_str);
    let expected_edit_handoffs = format!("Edit(/{}/handoffs/**)", resolved_str);
    assert!(
        allow_strs.contains(&expected_read_signals.as_str()),
        "Should have resolved .loom/work/signals read permission, got: {:?}",
        allow_strs
    );
    assert!(
        allow_strs.contains(&expected_edit_handoffs.as_str()),
        "Should have resolved .loom/work/handoffs edit permission, got: {:?}",
        allow_strs
    );

    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();
    let deny_admin = format!("Read(/{}/admin.token)", resolved_str);
    let deny_user = format!("Read(/{}/user.token)", resolved_str);
    assert!(deny_strs.contains(&deny_admin.as_str()));
    assert!(deny_strs.contains(&deny_user.as_str()));

    assert!(allow_strs.contains(&"Read(.loom/work/signals/**)"));
}

#[test]
fn test_merge_existing_permissions_empty() {
    // Existing file has no permissions block
    let existing = json!({
        "sandbox": {
            "enabled": true
        }
    });

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            allow_write: vec!["src/**".to_string()],
            deny_read: vec![],
            deny_write: vec![],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    let mut new_settings = generate_settings_json(&config);
    let original_allow_count = new_settings["permissions"]["allow"]
        .as_array()
        .unwrap()
        .len();

    // Merge should be a no-op
    merge_existing_permissions(&mut new_settings, &existing, false);

    let after_allow_count = new_settings["permissions"]["allow"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(original_allow_count, after_allow_count);
}

#[test]
fn test_target_is_worktree_detection() {
    // A `.worktrees` path component marks a worktree (no filesystem needed).
    assert!(target_is_worktree(Path::new(
        "/home/u/proj/.worktrees/stage-1"
    )));
    // A plain repo root is the main repo.
    assert!(!target_is_worktree(Path::new("/home/u/proj")));

    #[cfg(unix)]
    {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        let base = temp_dir.path();

        // A symlinked `.work` marks a worktree even without a `.worktrees`
        // component (the structural invariant loom relies on) — the legacy
        // layout arm.
        let real_work = base.join("real-work");
        fs::create_dir_all(&real_work).unwrap();
        let wt = base.join("checkout");
        fs::create_dir_all(&wt).unwrap();
        std::os::unix::fs::symlink(&real_work, wt.join(".work")).unwrap();
        assert!(target_is_worktree(&wt));

        // A real `.work` directory is the main repo.
        let main = base.join("main");
        fs::create_dir_all(main.join(".work")).unwrap();
        assert!(!target_is_worktree(&main));

        // A symlinked `.loom/work` under a real `.loom/` marks a worktree —
        // the nested layout arm.
        let real_nested_work = base.join("real-nested-work");
        fs::create_dir_all(&real_nested_work).unwrap();
        let nested_wt = base.join("nested-checkout");
        let nested_wt_loom = nested_wt.join(".loom");
        fs::create_dir_all(&nested_wt_loom).unwrap();
        std::os::unix::fs::symlink(&real_nested_work, nested_wt_loom.join("work")).unwrap();
        assert!(target_is_worktree(&nested_wt));

        // A real `.loom/` (holding no `work` symlink) alone must NOT mark a
        // worktree — both the main repo and a worktree have a real `.loom/`.
        let nested_main = base.join("nested-main");
        fs::create_dir_all(nested_main.join(".loom")).unwrap();
        assert!(!target_is_worktree(&nested_main));
    }
}

#[test]
fn test_write_settings_main_repo_strips_worktree_escape_denies() {
    use tempfile::TempDir;

    // A plain repo root (not under .worktrees, no `.work` symlink) is the main
    // repo: worktree-relative escape rules must be stripped, because `../..`
    // resolves to `$HOME` there and would deny the entire home directory.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        // FilesystemConfig::default() includes ../../** (deny_read also has
        // ../.worktrees/**). An explicit non-traversal entry stands in for a
        // plan-authored deny_write path, to prove non-traversal entries
        // survive alongside the stripped traversal ones.
        filesystem: FilesystemConfig {
            deny_write: {
                let mut deny_write = FilesystemConfig::default().deny_write;
                deny_write.push("some/plan/path/**".to_string());
                deny_write
            },
            ..FilesystemConfig::default()
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, repo_root).unwrap();

    let result: Value = serde_json::from_str(
        &fs::read_to_string(repo_root.join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !deny_strs.iter().any(|p| p.contains("../")),
        "main repo deny must not contain parent-traversal rules, got: {deny_strs:?}"
    );
    assert!(
        !deny_strs.iter().any(|p| p.contains(".worktrees")),
        "main repo deny must not reference .worktrees, got: {deny_strs:?}"
    );
    // Non-traversal protections survive.
    assert!(deny_strs.contains(&"Read(~/.ssh/**)"));
    assert!(deny_strs.contains(&"Edit(some/plan/path/**)"));
}

#[test]
fn test_write_settings_worktree_drops_escape_write_deny_from_edit_rule() {
    use tempfile::TempDir;

    // Even inside a real worktree (path under .worktrees/<stage>/), a
    // `../`-relative deny_write entry must NEVER be emitted as an
    // enforceable `Edit(...)` permission rule: `.worktrees/<stage>/../..`
    // resolves to an ancestor of the worktree, and Claude Code's `**`
    // crosses path separators, so `Edit(../../**)` matches the worktree's
    // OWN files (e.g. `<repo>/.worktrees/<stage>/loom/src/foo.rs`) and
    // deny wins over allow — refusing the agent's very first edit. This
    // config's `deny_write` carries both the traversal entry
    // (`default_deny_write()`'s only default now) and an explicit
    // non-traversal path standing in for a plan-authored deny_write
    // entry: only the traversal entry is dropped. Worktree write-escape
    // is still enforced independently by the OS sandbox's `allowOnly`
    // list and the worktree hooks.
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join(".worktrees").join("my-stage");
    fs::create_dir_all(&worktree_path).unwrap();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig {
            deny_read: vec![],
            deny_write: vec!["../../**".to_string(), "some/plan/path/**".to_string()],
            allow_write: vec![],
        },
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, &worktree_path).unwrap();

    let result: Value = serde_json::from_str(
        &fs::read_to_string(worktree_path.join(".claude/settings.local.json")).unwrap(),
    )
    .unwrap();
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !deny_strs.iter().any(|p| p.contains("../")),
        "worktree permissions.deny must not contain any parent-traversal \
         Edit(...) rule, got: {deny_strs:?}"
    );
    assert!(
        deny_strs.contains(&"Edit(some/plan/path/**)"),
        "the non-traversal deny_write entry must still be emitted, got: {deny_strs:?}"
    );
}

#[test]
fn test_write_settings_main_repo_drops_stale_escape_from_existing() {
    use tempfile::TempDir;

    // Simulate a main-repo settings.local.json written by an OLDER loom
    // version that leaked worktree-relative escape rules. Re-running the
    // generator on the main repo must scrub them (both Read and Write sides),
    // even though the merge preserves other user-approved permissions.
    //
    // `Write(~/.bashrc)` stands in for a legitimate user-authored deny
    // entry unrelated to loom's own rules — its INTENT must survive the
    // merge, pinning that loom does not silently discard rules it inherits
    // (mirrors the sibling fixture in
    // `test_write_settings_preserves_existing_deny_but_not_allow`). What
    // it survives as is `Edit(~/.bashrc)`: the `Write(...)` spelling is
    // inert at the tool layer, so carrying it verbatim would keep the
    // startup warning and none of the protection. A stale
    // `Write(doc/loom/knowledge/**)` is deliberately NOT used here: that
    // specific entry is dropped rather than migrated — see
    // `merge_existing_permissions`'s knowledge-dir carve-out, exercised
    // separately below.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path();
    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let stale = json!({
        "permissions": {
            "deny": [
                "Read(../../**)",
                "Read(../.worktrees/**)",
                "Write(../../**)",
                "Write(~/.bashrc)"
            ]
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&stale).unwrap(),
    )
    .unwrap();

    let config = MergedSandboxConfig {
        enabled: true,
        auto_allow: true,
        allow_unsandboxed_escape: false,
        excluded_commands: vec![],
        filesystem: FilesystemConfig::default(),
        network: NetworkConfig::default(),
        linux: LinuxConfig::default(),
        permission_mode: PermissionMode::Auto,
        implementers: Implementers::default(),
        command_confinement: CommandConfinement::default(),
    };

    write_settings(&config, repo_root).unwrap();

    let result: Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.local.json")).unwrap())
            .unwrap();
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !deny_strs
            .iter()
            .any(|p| p.contains("../") || p.contains(".worktrees")),
        "stale escape rules must be scrubbed from the main repo file, got: {deny_strs:?}"
    );
    // A legitimate, unrelated user-authored deny entry is preserved — in
    // the enforceable spelling.
    assert!(deny_strs.contains(&"Edit(~/.bashrc)"), "got: {deny_strs:?}");
    assert!(
        !deny_strs.iter().any(|p| p.starts_with("Write(")),
        "no inert Write(...) deny may survive regeneration, got: {deny_strs:?}"
    );
}

#[test]
fn test_write_settings_scrubs_stale_knowledge_dir_deny_from_existing() {
    use tempfile::TempDir;

    // A settings.local.json written before this fix could carry a
    // knowledge-dir deny in EITHER form: `Edit(...)` (the enforced form)
    // or `Write(...)` (parsed but inert at the tool layer, still leaks
    // into the OS sandbox's write denies). Without the carve-out in
    // `merge_existing_permissions`, `deny` is unioned with whatever is
    // already on disk, so either form would survive regeneration forever
    // and permanently block the `loom knowledge update` CLI subprocess
    // for that worktree — and the `Write(...)` one must not be rescued by
    // the Write->Edit migration either. An unrelated deny entry proves the
    // merge still carries an inherited rule's intent forward.
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path();
    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    let stale = json!({
        "permissions": {
            "deny": [
                "Edit(doc/loom/knowledge/**)",
                "Write(doc/loom/knowledge/**)",
                "Write(~/.bashrc)"
            ]
        }
    });
    fs::write(
        claude_dir.join("settings.local.json"),
        serde_json::to_string_pretty(&stale).unwrap(),
    )
    .unwrap();

    let config = default_config();
    write_settings(&config, worktree_path).unwrap();

    let result: Value =
        serde_json::from_str(&fs::read_to_string(claude_dir.join("settings.local.json")).unwrap())
            .unwrap();
    let deny = result["permissions"]["deny"].as_array().unwrap();
    let deny_strs: Vec<&str> = deny.iter().filter_map(|v| v.as_str()).collect();

    assert!(
        !deny_strs.iter().any(|p| p.contains("doc/loom/knowledge")),
        "neither Edit() nor Write() knowledge-dir deny may survive, got: {deny_strs:?}"
    );
    assert!(
        deny_strs.contains(&"Edit(~/.bashrc)"),
        "an unrelated inherited deny entry must still survive, migrated to \
         the enforceable spelling, got: {deny_strs:?}"
    );
}

#[test]
fn test_preserve_unowned_keys_carries_enabled_plugins_when_codex_licensed() {
    // Worktree-scoped: the codex-license gate applies here.
    let config = codex_licensed_config();
    let existing = json!({
        "enabledPlugins": { "codex": true }
    });
    let mut new_settings = json!({
        "sandbox": { "enabled": true }
    });

    preserve_unowned_keys(&mut new_settings, &existing, &config, true);

    assert_eq!(new_settings["enabledPlugins"], json!({ "codex": true }));
}

#[test]
fn test_preserve_unowned_keys_carries_extra_known_marketplaces_when_codex_licensed() {
    // Worktree-scoped: the codex-license gate applies here.
    let config = codex_licensed_config();
    let existing = json!({
        "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
    });
    let mut new_settings = json!({
        "sandbox": { "enabled": true }
    });

    preserve_unowned_keys(&mut new_settings, &existing, &config, true);

    assert_eq!(
        new_settings["extraKnownMarketplaces"],
        json!({ "codex-marketplace": "https://example.com" })
    );
}

#[test]
fn test_preserve_unowned_keys_claude_only_worktree_does_not_carry_enabled_plugins() {
    // SECURITY: `.claude/settings.local.json` lives inside the
    // agent-writable, respawn-reused WORKTREE, so carrying these keys
    // forward unconditionally would let a stage agent write its own
    // `enabledPlugins` entry and have loom carry it into the next
    // session. A claude-only worktree stage has no legitimate reason to
    // need either key, so the gate must block the carry-forward
    // entirely. See the non-worktree negative control below, where the
    // same claude-only config must NOT be gated.
    let config = default_config(); // Implementers::default() is claude-only.
    assert!(!config.implementers.includes_codex());

    let existing = json!({
        "enabledPlugins": { "codex": true },
        "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
    });
    let mut new_settings = json!({
        "sandbox": { "enabled": true }
    });

    preserve_unowned_keys(&mut new_settings, &existing, &config, true);

    assert!(new_settings.get("enabledPlugins").is_none());
    assert!(new_settings.get("extraKnownMarketplaces").is_none());
}

#[test]
fn test_preserve_unowned_keys_claude_only_non_worktree_carries_enabled_plugins() {
    // SECURITY (the flip side of the worktree test above): the main repo
    // root's settings.local.json is NOT agent-writable, so there is no
    // self-grant to defend against there, and the codex-license gate
    // must NOT apply. `loom repair --fix` writes the main repo as
    // claude-only (`Implementers::default()`) unconditionally
    // (`commands/repair.rs::fix_sandbox_settings`); before this fix that
    // deleted a pre-existing codex plugin install on every run.
    let config = default_config(); // Implementers::default() is claude-only.
    assert!(!config.implementers.includes_codex());

    let existing = json!({
        "enabledPlugins": { "codex": true },
        "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
    });
    let mut new_settings = json!({
        "sandbox": { "enabled": true }
    });

    preserve_unowned_keys(&mut new_settings, &existing, &config, false);

    assert_eq!(
        new_settings["enabledPlugins"],
        json!({ "codex": true }),
        "a non-worktree target must preserve enabledPlugins even for a \
         claude-only config, got: {new_settings:?}"
    );
    assert_eq!(
        new_settings["extraKnownMarketplaces"],
        json!({ "codex-marketplace": "https://example.com" }),
        "got: {new_settings:?}"
    );
}

#[test]
fn test_preserve_unowned_keys_noop_when_absent() {
    let config = codex_licensed_config();
    let existing = json!({
        "sandbox": { "enabled": false }
    });
    let mut new_settings = json!({
        "sandbox": { "enabled": true }
    });

    preserve_unowned_keys(&mut new_settings, &existing, &config, true);

    assert!(new_settings.get("enabledPlugins").is_none());
    assert!(new_settings.get("extraKnownMarketplaces").is_none());
    assert_eq!(new_settings["sandbox"]["enabled"], true);
}

#[test]
fn test_preserve_unowned_keys_does_not_override_generated_keys() {
    // A stale/user `sandbox` key in the existing file must never clobber
    // the freshly generated one - only keys ABSENT from new_settings are
    // carried over. Codex-licensed so the gate itself isn't what's under
    // test here.
    let config = codex_licensed_config();
    let existing = json!({
        "sandbox": { "enabled": false }
    });
    let mut new_settings = json!({
        "sandbox": { "enabled": true }
    });

    preserve_unowned_keys(&mut new_settings, &existing, &config, true);

    assert_eq!(new_settings["sandbox"]["enabled"], true);
}

#[test]
fn test_write_settings_round_trip_worktree_codex_licensed_preserves_enabled_plugins() {
    use tempfile::TempDir;

    // The bug this guards against: write_settings regenerates the whole
    // file from scratch, so a plugin enabled at local scope (enabledPlugins)
    // used to vanish from every worktree on the next regeneration. Fixed by
    // carrying it forward - for a WORKTREE target, only for a stage that
    // licenses the codex lane, which is the only lane that needs it (see
    // the claude-only worktree negative-control test below, and the
    // non-worktree tests further down for the other half of the gate).
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join(".worktrees").join("my-stage");
    fs::create_dir_all(&worktree_path).unwrap();

    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.local.json");

    let existing_settings = json!({
        "enabledPlugins": { "codex": true },
        "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" },
        "permissions": {
            "allow": ["Read(~/.ssh/config)"]
        }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&existing_settings).unwrap(),
    )
    .unwrap();

    let config = codex_licensed_config();

    write_settings(&config, &worktree_path).unwrap();

    let result: Value = serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

    assert_eq!(
        result["enabledPlugins"],
        json!({ "codex": true }),
        "enabledPlugins must survive settings regeneration, got: {result:?}"
    );
    assert_eq!(
        result["extraKnownMarketplaces"],
        json!({ "codex-marketplace": "https://example.com" }),
        "extraKnownMarketplaces must survive settings regeneration, got: {result:?}"
    );
}

#[test]
fn test_write_settings_round_trip_claude_only_worktree_drops_enabled_plugins() {
    use tempfile::TempDir;

    // SECURITY: a claude-only WORKTREE stage must NOT carry
    // `enabledPlugins` / `extraKnownMarketplaces` forward. Both keys live
    // in a file the stage agent itself can write, and worktrees are
    // reused across respawn / retry / crash recovery - an unconditional
    // carry-forward would let a claude-only stage agent self-grant
    // plugin enablement it was never licensed for. This is worktree-
    // scoped on purpose: the gate must NOT apply to a non-worktree
    // target (see the negative control immediately below), because that
    // target is not agent-writable and has nothing to self-grant.
    let temp_dir = TempDir::new().unwrap();
    let worktree_path = temp_dir.path().join(".worktrees").join("my-stage");
    fs::create_dir_all(&worktree_path).unwrap();

    let claude_dir = worktree_path.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.local.json");

    let existing_settings = json!({
        "enabledPlugins": { "codex": true },
        "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&existing_settings).unwrap(),
    )
    .unwrap();

    let config = default_config(); // Implementers::default() is claude-only.
    assert!(!config.implementers.includes_codex());

    write_settings(&config, &worktree_path).unwrap();

    let result: Value = serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

    assert!(
        result.get("enabledPlugins").is_none(),
        "claude-only worktree stage must not carry enabledPlugins forward, got: {result:?}"
    );
    assert!(
        result.get("extraKnownMarketplaces").is_none(),
        "claude-only worktree stage must not carry extraKnownMarketplaces forward, \
         got: {result:?}"
    );
}

#[test]
fn test_write_settings_round_trip_claude_only_non_worktree_preserves_enabled_plugins() {
    use tempfile::TempDir;

    // SECURITY (CRITICAL regression): the codex-license gate must apply
    // ONLY to worktree targets. `loom repair --fix`
    // (`commands/repair.rs::fix_sandbox_settings`) writes the MAIN repo's
    // settings.local.json unconditionally as claude-only
    // (`Implementers::default()`). Before this fix, gating the
    // carry-forward on `includes_codex()` regardless of target meant
    // `loom repair --fix` silently DELETED a legitimate codex plugin
    // install from the main repo on every run - because write_settings
    // regenerates the whole file from scratch. The main repo root is not
    // agent-writable, so there is no self-grant to defend against here.
    let temp_dir = TempDir::new().unwrap();
    let repo_root = temp_dir.path(); // not under .worktrees - the main repo root.

    let claude_dir = repo_root.join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    let settings_path = claude_dir.join("settings.local.json");

    let existing_settings = json!({
        "enabledPlugins": { "codex": true },
        "extraKnownMarketplaces": { "codex-marketplace": "https://example.com" }
    });
    fs::write(
        &settings_path,
        serde_json::to_string_pretty(&existing_settings).unwrap(),
    )
    .unwrap();

    let config = default_config(); // Implementers::default() is claude-only.
    assert!(!config.implementers.includes_codex());

    write_settings(&config, repo_root).unwrap();

    let result: Value = serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();

    assert_eq!(
        result["enabledPlugins"],
        json!({ "codex": true }),
        "a claude-only write to the MAIN repo must preserve a pre-existing \
         codex plugin install (e.g. `loom repair --fix`), got: {result:?}"
    );
    assert_eq!(
        result["extraKnownMarketplaces"],
        json!({ "codex-marketplace": "https://example.com" }),
        "got: {result:?}"
    );
}
