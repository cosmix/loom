//! The undo of a stopped trial's overlay: only a file still holding the
//! trial's replacement bytes goes back to its frozen copy.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::*;
use crate::models::stage::Stage;
use crate::verify::contracts::store::{write_freeze, FREEZE_RECORD_VERSION};

const FROZEN: &[u8] = b"frozen contract\n";
const REPLACED: &str = "replacement contract\n";

struct Fixture {
    _tmp: tempfile::TempDir,
    site: ContractSite,
    record: FreezeRecord,
}

impl Fixture {
    /// A freeze of `sub/contract.txt` under the working directory `wd` of the
    /// worktree.
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("work");
        let tree = tmp.path().join("tree");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(tree.join("wd/sub")).unwrap();
        let record = FreezeRecord {
            version: FREEZE_RECORD_VERSION,
            stage_id: "stage-a".into(),
            session_id: "session-a".into(),
            frozen_at: chrono::Utc::now(),
            base: "a".repeat(40),
            files: vec![FrozenFile {
                path: "sub/contract.txt".into(),
                sha256: sha256_hex(FROZEN),
            }],
            contracts: Vec::new(),
        };
        write_freeze(
            &work,
            &record,
            &[("sub/contract.txt".into(), FROZEN.to_vec())],
        )
        .unwrap();
        let stage = Stage {
            working_dir: Some("wd".into()),
            ..Stage::default()
        };
        let site = ContractSite::resolved(work, stage, tree.clone(), tree.join("wd"));
        Self {
            _tmp: tmp,
            site,
            record,
        }
    }

    fn file(&self) -> PathBuf {
        self.site.worktree_root.join("wd/sub/contract.txt")
    }

    fn write(&self, bytes: &[u8], mode: u32) {
        std::fs::write(self.file(), bytes).unwrap();
        std::fs::set_permissions(self.file(), std::fs::Permissions::from_mode(mode)).unwrap();
    }

    fn undo(&self, content: &str) -> Result<Vec<String>> {
        let replacements = [FileReplacement {
            file: "sub/contract.txt".into(),
            content: content.into(),
            base_sha256: None,
        }];
        undo_trial_overlay(&self.site, &self.record, &replacements)
    }
}

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn a_file_left_overlaid_goes_back_to_its_frozen_copy() {
    let fx = Fixture::new();
    fx.write(REPLACED.as_bytes(), 0o640);

    let restored = fx.undo(REPLACED).unwrap();

    assert_eq!(restored, ["wd/sub/contract.txt"]);
    assert_eq!(std::fs::read(fx.file()).unwrap(), FROZEN);
    assert_eq!(mode(&fx.file()), 0o640, "the file keeps its mode");
}

#[test]
fn a_file_without_the_replacement_bytes_is_left_alone() {
    let fx = Fixture::new();
    for bytes in [FROZEN, b"an edit after the trial\n".as_slice()] {
        fx.write(bytes, 0o644);

        assert!(fx.undo(REPLACED).unwrap().is_empty());
        assert_eq!(std::fs::read(fx.file()).unwrap(), bytes);
    }
}

#[test]
fn a_replacement_the_freeze_now_holds_is_left_alone() {
    let fx = Fixture::new();
    fx.write(FROZEN, 0o644);
    let frozen = std::str::from_utf8(FROZEN).unwrap();

    assert!(fx.undo(frozen).unwrap().is_empty());
    assert_eq!(std::fs::read(fx.file()).unwrap(), FROZEN);
}

#[test]
fn a_symlinked_target_is_refused_and_its_target_untouched() {
    let fx = Fixture::new();
    let outside = fx.site.work_dir.join("outside.txt");
    std::fs::write(&outside, REPLACED).unwrap();
    std::os::unix::fs::symlink(&outside, fx.file()).unwrap();

    assert!(fx.undo(REPLACED).is_err());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), REPLACED);
}
