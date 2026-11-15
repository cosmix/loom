use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

use super::excluded;
use crate::context::source_graph::body_hash;
use crate::context::source_graph::read::read_source;
use crate::git::revision::require_head_commit;
use crate::git::status_entries::{parse_porcelain_v1_z, StatusEntry, PORCELAIN_V1_Z};
use crate::git::worktree::WorktreeGit;

pub(crate) struct WorkingTree {
    pub head: String,
    /// sha256 hex over `head`, then one line per dirty path: `<xy> <path> <content_hash|deleted>`.
    pub generation: String,
    /// Path → status code from `git status --porcelain=v1 -z --untracked-files=all` (renames yield both paths).
    pub dirty: BTreeMap<String, String>,
}

pub(crate) fn working_tree(project_root: &Path) -> Result<WorkingTree> {
    let git = WorktreeGit::pinned_checkout(project_root)?;
    let head = require_head_commit(&git)?;
    let status = git.run_checked(&PORCELAIN_V1_Z)?;
    let dirty = dirty_paths(parse_porcelain_v1_z(status.as_bytes())?);
    let generation = generation(project_root, &head, &dirty)?;
    Ok(WorkingTree {
        head,
        generation,
        dirty,
    })
}

pub(crate) fn clean_generation(head: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(head.as_bytes());
    hasher.update(b"\n");
    hex::encode(hasher.finalize())
}

fn generation(project_root: &Path, head: &str, dirty: &BTreeMap<String, String>) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(head.as_bytes());
    hasher.update(b"\n");
    for (path, status) in dirty {
        let full_path = project_root.join(path);
        // A dirty path that exists but cannot be read (e.g. permission bits,
        // or a symlink the sandboxed worktree planted - never followed, see
        // `read_bounded`) must not fail the whole refresh - fold the refusal
        // into the identity so the generation still hashes deterministically,
        // mirroring how `layer::read_bytes` treats an unreadable file. The
        // marker is hashed rather than embedding the raw error (which could
        // vary by platform or leak path detail), while still changing the
        // generation if the refusal state itself changes.
        let identity = match read_source(project_root, Path::new(path)) {
            Ok(source) => source.hash,
            Err(_) if !full_path.exists() => "deleted".to_string(),
            Err(_) => body_hash(format!("{path}\0unreadable").as_bytes()),
        };
        hasher.update(format!("{status} {path} {identity}\n").as_bytes());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn dirty_paths(entries: Vec<StatusEntry>) -> BTreeMap<String, String> {
    let mut dirty = BTreeMap::new();
    for entry in entries {
        for path in std::iter::once(&entry.path).chain(entry.origin.iter()) {
            if !excluded(path) {
                dirty.insert(path.clone(), entry.code_str().to_owned());
            }
        }
    }
    dirty
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_dirty(output: &str) -> BTreeMap<String, String> {
        dirty_paths(parse_porcelain_v1_z(output.as_bytes()).unwrap())
    }

    #[test]
    fn generation_hash_preserves_status_spaces_renames_and_deleted_paths() {
        let temp = tempfile::TempDir::new().unwrap();
        std::fs::write(temp.path().join("new name.rs"), "fn renamed() {}\n").unwrap();
        std::fs::write(temp.path().join("next.rs"), "next\n").unwrap();
        let dirty = parsed_dirty("R  new name.rs\0old name.rs\0 M next.rs\0");

        assert_eq!(
            generation(temp.path(), "deadbeef", &dirty).unwrap(),
            "e63e2beec53251811c215f11221aebcc0bea249a02c9ab34e5b45410add99ae3"
        );
    }

    #[test]
    fn dirty_paths_on_empty_input_yields_no_dirty_paths() {
        assert!(parsed_dirty("").is_empty());
    }

    #[test]
    fn dirty_paths_reads_a_plain_modification() {
        let dirty = parsed_dirty("M  file.rs\0");
        assert_eq!(dirty.get("file.rs"), Some(&"M ".to_string()));
        assert_eq!(dirty.len(), 1);
    }

    #[test]
    fn dirty_paths_reads_an_untracked_entry() {
        let dirty = parsed_dirty("?? new_file.txt\0");
        assert_eq!(dirty.get("new_file.txt"), Some(&"??".to_string()));
        assert_eq!(dirty.len(), 1);
    }

    #[test]
    fn generation_treats_a_dirty_path_missing_from_disk_as_deleted() {
        // The old side of a rename no longer exists on disk. `generation`
        // must fold it into the hash via the "deleted" identity rather than
        // silently reusing content that happens to still pass `fs::read`.
        let temp = tempfile::TempDir::new().unwrap();
        let mut dirty = BTreeMap::new();
        dirty.insert("gone.rs".to_string(), "R ".to_string());
        let head = "deadbeef";

        let got = generation(temp.path(), head, &dirty).unwrap();

        let mut hasher = Sha256::new();
        hasher.update(head.as_bytes());
        hasher.update(b"\n");
        hasher.update(b"R  gone.rs deleted\n");
        let want = hex::encode(hasher.finalize());

        assert_eq!(got, want);
    }

    #[test]
    fn dirty_paths_rename_record_consumes_both_paths_without_shifting_the_next_record() {
        // A rename occupies two NUL-separated fields: the destination path
        // (with the "R " status prefix) then the bare source path. Getting
        // this wrong would shift every following record by one.
        let input = "R  new name.rs\0old name.rs\0 M next.rs\0";
        let dirty = parsed_dirty(input);
        assert_eq!(dirty.get("new name.rs"), Some(&"R ".to_string()));
        assert_eq!(dirty.get("old name.rs"), Some(&"R ".to_string()));
        assert_eq!(
            dirty.get("next.rs"),
            Some(&" M".to_string()),
            "the record after a rename must not be shifted or swallowed"
        );
        assert_eq!(dirty.len(), 3);
    }

    #[test]
    fn dirty_paths_excludes_each_side_of_renames_and_copies_independently() {
        let dirty =
            parsed_dirty("R  visible name.rs\0target/old name.rs\0 C .loom/new.rs\0old name.rs\0");
        assert_eq!(
            dirty,
            BTreeMap::from([
                ("old name.rs".to_owned(), " C".to_owned()),
                ("visible name.rs".to_owned(), "R ".to_owned()),
            ])
        );
    }
}
