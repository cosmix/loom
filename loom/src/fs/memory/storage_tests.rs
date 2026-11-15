use super::*;
use crate::fs::memory::types::MemoryEntryType;
use std::os::unix::fs::symlink;

#[test]
fn append_entry_refuses_a_symlinked_journal() {
    let work = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let target = outside.path().join("target.md");
    std::fs::write(&target, "untouched\n").unwrap();
    init_memory_dir(work.path()).unwrap();
    symlink(&target, memory_file_path(work.path(), "s")).unwrap();

    let entry = MemoryEntry::new(MemoryEntryType::Note, "a note".to_string());
    assert!(append_entry(work.path(), "s", &entry).is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "untouched\n");
}
