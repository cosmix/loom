use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use crate::models::stage::Stage;
use crate::verify::contracts::store::load_freeze;

/// Whether every changed path is present at its current frozen digest.
/// An empty change set is never covered by a freeze record.
pub(crate) fn only_intact_frozen(
    changed: &[String],
    current: &BTreeMap<String, String>,
    frozen: &BTreeMap<String, String>,
) -> bool {
    !changed.is_empty()
        && changed.iter().all(|path| {
            current
                .get(path)
                .is_some_and(|digest| digest != "deleted" && frozen.get(path) == Some(digest))
        })
}

/// The stage's freeze record keyed by worktree-relative path.
/// A missing or unreadable record gives no coverage.
pub(crate) fn frozen_digests(stage: &Stage, work_dir: &Path) -> Option<BTreeMap<String, String>> {
    let record = load_freeze(work_dir, &stage.id).ok()??;
    let declared = Path::new(stage.working_dir.as_deref().unwrap_or("."));
    if declared.is_absolute()
        || declared
            .components()
            .any(|part| part == Component::ParentDir)
    {
        return None;
    }
    let prefix: PathBuf = declared
        .components()
        .filter(|part| matches!(part, Component::Normal(_)))
        .collect();
    Some(
        record
            .files
            .into_iter()
            .map(|file| {
                let path = prefix.join(file.path);
                (path.to_string_lossy().into_owned(), file.sha256)
            })
            .collect(),
    )
}

#[cfg(test)]
pub(super) mod test_support {
    use super::*;
    use crate::utils::sha256_hex;
    use crate::verify::contracts::store::{
        write_freeze, FreezeRecord, FrozenFile, FREEZE_RECORD_VERSION,
    };

    pub(in crate::verify::review) fn freeze(
        stage: &Stage,
        work_dir: &Path,
        path: &str,
        content: &[u8],
    ) {
        let record = FreezeRecord {
            version: FREEZE_RECORD_VERSION,
            stage_id: stage.id.clone(),
            session_id: "session".to_string(),
            frozen_at: chrono::Utc::now(),
            base: "base".to_string(),
            files: vec![FrozenFile {
                path: path.to_string(),
                sha256: sha256_hex(content),
            }],
            contracts: Vec::new(),
        };
        write_freeze(work_dir, &record, &[(path.to_string(), content.to_vec())]).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::{test_support::freeze, *};
    use crate::utils::sha256_hex;
    use tempfile::TempDir;

    #[test]
    fn coverage_requires_each_changed_path_to_be_present_at_its_frozen_digest() {
        let path = "tests/c.rs".to_string();
        let source = "src/lib.rs".to_string();
        let digest = sha256_hex(b"frozen");
        let frozen = BTreeMap::from([(path.clone(), digest.clone())]);
        let mut current = frozen.clone();
        current.insert(source.clone(), sha256_hex(b"source"));
        assert!(only_intact_frozen(
            std::slice::from_ref(&path),
            &current,
            &frozen
        ));
        assert!(!only_intact_frozen(
            &[path.clone(), source],
            &current,
            &frozen
        ));
        assert!(!only_intact_frozen(&[], &current, &frozen));
        for value in [sha256_hex(b"edited"), "deleted".to_string()] {
            current.insert(path.clone(), value);
            assert!(!only_intact_frozen(
                std::slice::from_ref(&path),
                &current,
                &frozen
            ));
        }
        current.remove(&path);
        assert!(!only_intact_frozen(&[path], &current, &frozen));
    }

    #[test]
    fn freeze_paths_are_relative_to_the_worktree() {
        let work_dir = TempDir::new().unwrap();
        let mut stage = Stage {
            id: "s1".to_string(),
            ..Stage::default()
        };
        let path = "tests/c.rs";
        let content = b"frozen";
        assert_eq!(frozen_digests(&stage, work_dir.path()), None);
        freeze(&stage, work_dir.path(), path, content);
        for (working_dir, expected_path) in [
            (None, "tests/c.rs"),
            (Some("."), "tests/c.rs"),
            (Some("./packages//app/."), "packages/app/tests/c.rs"),
        ] {
            stage.working_dir = working_dir.map(str::to_string);
            assert_eq!(
                frozen_digests(&stage, work_dir.path()),
                Some(BTreeMap::from([(
                    expected_path.to_string(),
                    sha256_hex(content)
                )]))
            );
        }
        std::fs::write(work_dir.path().join("contracts/s1/freeze.json"), "{").unwrap();
        assert_eq!(frozen_digests(&stage, work_dir.path()), None);
    }
}
