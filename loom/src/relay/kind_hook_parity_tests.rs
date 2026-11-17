//! `RequestKind::is_control` and the relay hooks' shell copy of it
//! (`loom-hooks/_relay_kinds.sh::relay_kind_is_control`) must name the same kinds.

use super::RequestKind;
use crate::fs::permissions::constants::HOOK_RELAY_KINDS;
use std::process::Command;

#[test]
fn relay_kind_is_control_agrees_with_is_control_for_every_wire_name() {
    let dir = tempfile::tempdir().unwrap();
    let library = dir.path().join("_relay_kinds.sh");
    std::fs::write(&library, HOOK_RELAY_KINDS).unwrap();
    for kind in RequestKind::all() {
        let wire = kind.to_string();
        let status = Command::new("bash")
            .arg("-c")
            .arg(format!(
                "source '{}'; relay_kind_is_control '{wire}'",
                library.display()
            ))
            .status()
            .unwrap();
        assert_eq!(
            status.success(),
            kind.is_control(),
            "hook and RequestKind::is_control disagree on {wire}"
        );
    }
}
