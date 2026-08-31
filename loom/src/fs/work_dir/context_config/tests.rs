use super::*;
use std::fs;
use tempfile::TempDir;

use crate::fs::work_dir::{read_context_config, resolve_context_ceiling_tokens};

fn init_work(temp: &TempDir) -> std::path::PathBuf {
    let work = temp.path().join(".work");
    fs::create_dir_all(&work).unwrap();
    work
}

/// The built-in defaults are both 80% of a 1M-token model window — pinned
/// here as literals so a change to the fraction or the window shows up as a
/// test failure naming the values, not just a passing symbolic assertion.
#[test]
fn built_in_defaults_are_80_percent_of_a_1m_window() {
    assert_eq!(DEFAULT_CONTEXT_CEILING_TOKENS, 800_000);
    assert_eq!(DEFAULT_SUBAGENT_CEILING_TOKENS, 800_000);
}

#[test]
fn model_window_tokens_derives_both_ceilings_via_the_same_fraction() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);

    fs::write(
        work.join("config.toml"),
        "[context]\nmodel_window_tokens = 200000\n",
    )
    .unwrap();

    let config = read_context_config(&work).unwrap();
    assert_eq!(config.ceiling_tokens, 160_000);
    assert_eq!(config.subagent_ceiling_tokens, 160_000);
    assert_eq!(config.model_window_tokens, Some(200_000));

    // A stage override still wins over a window-derived ceiling.
    assert_eq!(resolve_context_ceiling_tokens(&work, Some(42_000)), 42_000);
}

#[test]
fn explicit_ceiling_wins_over_a_model_window_derivation() {
    let temp = TempDir::new().unwrap();
    let work = init_work(&temp);

    fs::write(
        work.join("config.toml"),
        "[context]\nmodel_window_tokens = 200000\nceiling_tokens = 999\n",
    )
    .unwrap();

    let config = read_context_config(&work).unwrap();
    // Explicit ceiling_tokens is honored verbatim...
    assert_eq!(config.ceiling_tokens, 999);
    // ...but subagent_ceiling_tokens, left unset, still derives from the window.
    assert_eq!(config.subagent_ceiling_tokens, 160_000);
}

/// The daemon backstop clamps `ceiling x DAEMON_CEILING_MULTIPLIER` to
/// `window x DAEMON_BACKSTOP_WINDOW_FRACTION`. At the built-in 800,000
/// ceiling the multiplier alone lands exactly on the window (1,000,000);
/// the clamp is what keeps the backstop reachable.
#[test]
fn backstop_tokens_clamps_to_the_window_when_the_multiplier_would_reach_it() {
    let config = ContextConfig::default();
    assert_eq!(config.ceiling_tokens, 800_000);
    assert_eq!(config.backstop_tokens(config.ceiling_tokens), 950_000);
}

/// Below that clamp point, the multiplier still governs — the clamp must not
/// silently lower every backstop to the window fraction regardless of ceiling.
#[test]
fn backstop_tokens_uses_the_multiplier_when_it_stays_under_the_window_clamp() {
    let config = ContextConfig::default();
    assert_eq!(config.backstop_tokens(100_000), 125_000);
}

/// A custom `model_window_tokens` clamps against ITS window, not the built-in
/// one.
#[test]
fn backstop_tokens_clamps_against_a_custom_model_window() {
    let config = ContextConfig {
        ceiling_tokens: 160_000,
        subagent_ceiling_tokens: 160_000,
        model_window_tokens: Some(200_000),
    };
    // 160_000 * 1.25 = 200_000, but 200_000 * 0.95 = 190_000 clamps it.
    assert_eq!(config.backstop_tokens(160_000), 190_000);
}
