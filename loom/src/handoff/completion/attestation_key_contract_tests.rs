//! Contract `group-writable-attestation-key-accepted`: the completion
//! attestation key is refused when its group or world write bit is set, since
//! whoever can rewrite it can forge completion evidence.

use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use super::{attestation_key, ATTESTATION_KEY_FILE};

#[test]
fn a_group_or_world_writable_key_is_refused() {
    for (mode, refused) in [(0o620, true), (0o602, true), (0o600, false)] {
        let work_dir = TempDir::new().unwrap();
        let key = work_dir.path().join(ATTESTATION_KEY_FILE);
        std::fs::write(&key, [7u8; 32]).unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(mode)).unwrap();

        let result = attestation_key(work_dir.path());

        assert_eq!(
            result.is_err(),
            refused,
            "mode {mode:o}: expected refused={refused}, got {:?}",
            result.err()
        );
    }
}
