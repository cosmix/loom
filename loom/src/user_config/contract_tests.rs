//! Contract test: the retired `[terminal]` user setting is ignored, never
//! validated, so the settings beside it stay in force.

use crate::user_config::keys::spec;
use crate::user_config::parse_document;

#[test]
fn retired_terminal_key_leaves_other_settings_in_force() {
    let config = parse_document("[terminal]\nbackend = \"kitty\"\n\n[update]\ncheck = false\n")
        .expect("a retired [terminal] section must not fail the document");
    assert!(
        !config.update_check(),
        "[update] check = false must stay in force beside a retired [terminal] section"
    );
    assert!(
        spec("terminal.backend").is_err(),
        "terminal.backend must no longer be a registered key"
    );
}
