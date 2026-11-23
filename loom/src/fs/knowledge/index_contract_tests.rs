//! Contract: `write_index` renders the index from the tree it sees once it
//! holds the knowledge directory lock, so a topic written by a lock holder
//! while the index writer waited appears in the INDEX.md it writes.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use tempfile::TempDir;

use crate::fs::knowledge::index::write_index;
use crate::fs::locking::locked_dir_update;

/// Whether some table row of `index` names `word` as a whole word.
fn topic_row_mentions(index: &str, word: &str) -> bool {
    index
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .any(|line| {
            line.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|token| token.eq_ignore_ascii_case(word))
        })
}

#[test]
fn index_generation_waits_for_the_directory_lock() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();
    write_index(&root).unwrap();

    let writer: JoinHandle<anyhow::Result<()>> = locked_dir_update(&root, || {
        let (started_tx, started_rx) = mpsc::channel();
        let thread_root = root.clone();
        let handle = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            write_index(&thread_root)
        });
        started_rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let dir = root.join("architecture");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("late.md"),
            "# Late\n\n> Added while the writer waited.\n",
        )
        .unwrap();
        Ok(handle)
    })
    .unwrap();
    writer.join().unwrap().unwrap();

    let index = std::fs::read_to_string(root.join("INDEX.md")).unwrap();
    assert!(
        topic_row_mentions(&index, "late"),
        "INDEX.md written after the lock was released lacks the topic created while \
         the writer waited:\n{index}"
    );
}
