use super::*;

#[test]
fn missing_config_lacks_exclusion_and_ensure_creates_it() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    assert!(!codex_config_excludes_slash_tmp(&path));
    assert!(ensure_codex_config_excludes_slash_tmp(&path).unwrap());
    assert!(codex_config_excludes_slash_tmp(&path));
    // Idempotent: a second ensure changes nothing.
    assert!(!ensure_codex_config_excludes_slash_tmp(&path).unwrap());
}

#[test]
fn ensure_preserves_comments_and_unrelated_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "# user comment\nmodel = \"gpt-6-sol\"\n\n[mcp_servers.vnkt]\nurl = \"https://vnkt.org/mcp\"\n",
    )
    .unwrap();
    assert!(ensure_codex_config_excludes_slash_tmp(&path).unwrap());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("# user comment"));
    assert!(written.contains("model = \"gpt-6-sol\""));
    assert!(written.contains("[mcp_servers.vnkt]"));
    assert!(codex_config_excludes_slash_tmp(&path));
}

#[test]
fn explicit_false_is_detected_and_flipped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "[sandbox_workspace_write]\nnetwork_access = true\nexclude_slash_tmp = false\n",
    )
    .unwrap();
    assert!(!codex_config_excludes_slash_tmp(&path));
    assert!(ensure_codex_config_excludes_slash_tmp(&path).unwrap());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("network_access = true"));
    assert!(codex_config_excludes_slash_tmp(&path));
}

#[test]
fn unparseable_config_is_never_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "not [ valid toml").unwrap();
    let err = ensure_codex_config_excludes_slash_tmp(&path).unwrap_err();
    assert!(err.to_string().contains("unparseable"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "not [ valid toml",
        "a file loom cannot parse must be left untouched"
    );
}

#[test]
fn codex_model_candidates_newest_sol_falls_back_to_older_sol() {
    assert_eq!(
        codex_model_candidates("gpt-6.1-sol"),
        ["gpt-6.1-sol", "gpt-6-sol"]
    );
}

#[test]
fn codex_model_candidates_older_sol_tries_newer_sol_next() {
    assert_eq!(
        codex_model_candidates("gpt-6-sol"),
        ["gpt-6-sol", "gpt-6.1-sol"]
    );
}

#[test]
fn codex_model_candidates_single_member_families_never_mix_tiers() {
    for model in ["gpt-6-astra", "gpt-6.1-terra", "gpt-6-luna"] {
        assert_eq!(codex_model_candidates(model), [model]);
    }
}

#[test]
fn codex_model_candidates_unknown_id_is_returned_alone() {
    // Parseable and in the sol family, but not an accepted model.
    assert_eq!(codex_model_candidates("gpt-6.2-sol"), ["gpt-6.2-sol"]);
    assert_eq!(codex_model_candidates("o3"), ["o3"]);
    assert_eq!(codex_model_candidates(""), [""]);
}

#[test]
fn codex_model_candidates_default_is_the_newest_sol() {
    assert_eq!(
        codex_model_candidates(DEFAULT_PRESSURE_CODEX_MODEL)[0],
        DEFAULT_PRESSURE_CODEX_MODEL
    );
    assert!(CODEX_MODELS.contains(&DEFAULT_PRESSURE_CODEX_MODEL));
}

#[test]
fn codex_model_candidates_version_compare_pads_missing_segments() {
    use std::cmp::Ordering;
    assert_eq!(compare_versions(&[6], &[6, 1]), Ordering::Less);
    assert_eq!(compare_versions(&[5, 6], &[6]), Ordering::Less);
    assert_eq!(compare_versions(&[6], &[6, 0]), Ordering::Equal);
    assert_eq!(compare_versions(&[6, 1], &[6]), Ordering::Greater);
}

#[test]
fn codex_model_candidates_parse_rejects_ids_outside_the_shape() {
    assert_eq!(
        parse_codex_model("gpt-6.1-terra"),
        Some((vec![6, 1], "terra"))
    );
    assert_eq!(parse_codex_model("gpt-sol"), None);
    assert_eq!(parse_codex_model("gpt-6-"), None);
    assert_eq!(parse_codex_model("gpt-6.x-sol"), None);
    assert_eq!(parse_codex_model("claude-6-sol"), None);
}
#[test]
fn companion_preflight_requires_the_supported_regular_file() {
    use std::os::unix::fs::symlink;

    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".claude/plugins/cache/openai-codex/codex");
    let supported = root
        .join(super::companion_version().unwrap())
        .join("scripts/codex-companion.mjs");
    let unsupported = root.join("99.0.0/scripts/codex-companion.mjs");
    assert!(!super::codex_companion_installed_in(home.path()));
    std::fs::create_dir_all(unsupported.parent().unwrap()).unwrap();
    std::fs::write(&unsupported, "other").unwrap();
    assert!(!super::codex_companion_installed_in(home.path()));
    std::fs::create_dir_all(supported.parent().unwrap()).unwrap();
    symlink(&unsupported, &supported).unwrap();
    assert!(!super::codex_companion_installed_in(home.path()));
    std::fs::remove_file(&supported).unwrap();
    std::fs::write(&supported, "supported").unwrap();
    assert!(super::codex_companion_installed_in(home.path()));
    std::fs::remove_file(unsupported).unwrap();
    assert!(super::codex_companion_installed_in(home.path()));
}
