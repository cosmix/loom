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
        "# user comment\nmodel = \"gpt-6.2-sol\"\n\n[mcp_servers.vnkt]\nurl = \"https://vnkt.org/mcp\"\n",
    )
    .unwrap();
    assert!(ensure_codex_config_excludes_slash_tmp(&path).unwrap());
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(written.contains("# user comment"));
    assert!(written.contains("model = \"gpt-6.2-sol\""));
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

/// A synthetic list with multi-member families; the real [`CODEX_MODELS`] has
/// one member per family.
const SYNTHETIC: &[&str] = &[
    "gpt-8-vega",
    "gpt-7.1-vega",
    "gpt-7-vega",
    "gpt-6-vega",
    "gpt-6.1-terra",
    "gpt-6-luna",
];

#[test]
fn candidates_in_newest_member_falls_back_to_older_members_descending() {
    assert_eq!(
        candidates_in(SYNTHETIC, "gpt-8-vega"),
        ["gpt-8-vega", "gpt-7.1-vega", "gpt-7-vega", "gpt-6-vega"]
    );
}

#[test]
fn candidates_in_middle_member_tries_nearest_newer_then_nearest_older() {
    assert_eq!(
        candidates_in(SYNTHETIC, "gpt-7-vega"),
        ["gpt-7-vega", "gpt-7.1-vega", "gpt-8-vega", "gpt-6-vega"]
    );
}

#[test]
fn candidates_in_oldest_member_tries_newer_members_ascending() {
    assert_eq!(
        candidates_in(SYNTHETIC, "gpt-6-vega"),
        ["gpt-6-vega", "gpt-7-vega", "gpt-7.1-vega", "gpt-8-vega"]
    );
}

#[test]
fn candidates_in_never_mixes_tiers() {
    for model in ["gpt-6.1-terra", "gpt-6-luna"] {
        assert_eq!(candidates_in(SYNTHETIC, model), [model]);
    }
}

#[test]
fn candidates_in_unknown_id_is_returned_alone() {
    // Parseable and in the vega family, but absent from the list.
    assert_eq!(candidates_in(SYNTHETIC, "gpt-9-vega"), ["gpt-9-vega"]);
    assert_eq!(candidates_in(SYNTHETIC, "o3"), ["o3"]);
    assert_eq!(candidates_in(SYNTHETIC, ""), [""]);
}

#[test]
fn codex_model_candidates_every_real_family_has_one_member() {
    let mut families = std::collections::HashSet::new();
    for model in CODEX_MODELS {
        let (_, family) = parse_codex_model(model).expect("listed ids parse");
        assert!(families.insert(family), "family {family} listed twice");
        assert_eq!(codex_model_candidates(model), [*model]);
    }
}

#[test]
fn codex_model_candidates_unknown_id_is_returned_alone() {
    assert_eq!(codex_model_candidates("gpt-6.3-sol"), ["gpt-6.3-sol"]);
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
    assert_eq!(parse_codex_model("gpt-6.2-sol"), Some((vec![6, 2], "sol")));
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

#[test]
fn lane_selection_respects_backend_and_qualification() {
    use super::backend::lane_from;
    assert_eq!(
        lane_from(CodexBackend::Legacy, true, Ok(()), || panic!(
            "unexpected probe"
        )),
        CodexLane::Legacy
    );
    assert_eq!(
        lane_from(
            CodexBackend::Legacy,
            true,
            Err("missing plugin".into()),
            || panic!("unexpected probe"),
        ),
        CodexLane::Unavailable("missing plugin".into())
    );
    assert_eq!(
        lane_from(CodexBackend::Native, false, Ok(()), || panic!(
            "unexpected probe"
        )),
        CodexLane::Unavailable(CODEX_CLI_MISSING.into())
    );
    assert_eq!(
        lane_from(
            CodexBackend::Native,
            true,
            Err("missing plugin".into()),
            || Ok(()),
        ),
        CodexLane::Native
    );
    assert_eq!(
        lane_from(CodexBackend::Native, true, Ok(()), || Err(
            "no pid namespace".into()
        )),
        CodexLane::Unavailable("no pid namespace".into())
    );
}

#[test]
fn configured_backend_resolves_project_and_user_tiers() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    assert_eq!(configured_backend(&workspace), CodexBackend::Native);
    let config = workspace.join("config.toml");
    std::fs::write(&config, "[codex]\nbackend = \"legacy\"\n").unwrap();
    assert_eq!(configured_backend(&workspace), CodexBackend::Legacy);
    let user = dir.path().join("user.toml");
    std::fs::write(&user, "[codex]\nbackend = \"legacy\"\n").unwrap();
    let _redirect = crate::user_config::redirect_user_config(user);
    std::fs::write(&config, "[codex]\nbackend = \"bogus\"\n").unwrap();
    assert_eq!(configured_backend(&workspace), CodexBackend::Legacy);
    std::fs::write(&config, "[codex]\nbackend = \"native\"\n").unwrap();
    assert_eq!(configured_backend(&workspace), CodexBackend::Native);
}
