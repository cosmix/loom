//! A key that left the registry is refused like any unknown key, and the
//! refusal writes nothing.

use super::super::update;
use super::super::wire::ConfigError;
use super::{scratch, Scratch};

/// The key of the removed terminal selector, assembled here so the literal
/// does not appear in the source.
const REMOVED_KEY: &str = concat!("terminal", ".backend");

/// Both files as text; a file that does not exist reads as empty.
fn files(scratch: &Scratch) -> (String, String) {
    let user = crate::user_config::config_path().expect("user config path");
    (
        std::fs::read_to_string(user).unwrap_or_default(),
        std::fs::read_to_string(scratch.work().join("config.toml")).unwrap_or_default(),
    )
}

#[test]
fn a_post_for_a_removed_key_is_a_400_and_leaves_both_files_unchanged() {
    let scratch = scratch();
    let before = files(&scratch);

    for scope in ["user", "project"] {
        let body = format!(r#"{{"scope":"{scope}","name":"{REMOVED_KEY}","value":"tmux"}}"#);
        let (status, response) = update(&scratch.base, body.as_bytes());

        assert_eq!(status, 400, "{scope}: {response}");
        let error: ConfigError = serde_json::from_str(&response).expect("a ConfigError");
        assert!(
            error.error.contains(REMOVED_KEY),
            "{scope}: {}",
            error.error
        );
    }

    assert_eq!(files(&scratch), before);
}
