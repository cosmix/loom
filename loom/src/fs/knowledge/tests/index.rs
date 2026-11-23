use super::*;
use crate::fs::knowledge::dir::KnowledgeDir;
use std::fs::{FileTimes, OpenOptions};
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

#[test]
fn write_index_skips_an_identical_file() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    fs::create_dir_all(root).unwrap();
    write_index(root).unwrap();
    let path = root.join(INDEX_FILENAME);
    let fixed = SystemTime::UNIX_EPOCH + Duration::from_secs(1_234_567);
    OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(fixed))
        .unwrap();
    let before = fs::metadata(&path).unwrap().modified().unwrap();

    write_index(root).unwrap();

    let after = fs::metadata(path).unwrap().modified().unwrap();
    assert_eq!(after, before);
}

#[test]
fn test_generate_index_lists_tier1_and_topics_with_marker() {
    let temp = TempDir::new().unwrap();
    let project_root = temp.path();
    let knowledge = KnowledgeDir::new(project_root);
    knowledge.initialize().unwrap();

    let arch_dir = knowledge.root().join("architecture");
    fs::create_dir_all(&arch_dir).unwrap();
    fs::write(
        arch_dir.join("merge-flow.md"),
        "# Merge Flow\n\n> Merge details.\n",
    )
    .unwrap();

    let content = generate_index(knowledge.root()).unwrap();
    assert!(content.starts_with(GENERATED_MARKER));
    assert!(content.contains("architecture.md"));
    assert!(content.contains("architecture/merge-flow.md"));
    assert!(content.contains("[merge-flow]"));
    assert!(content.contains("Merge details."));
    // The current header sentence, asserted positively. Superseded
    // wordings are deliberately NOT quoted here to assert their absence:
    // a stage truth check greps this file for exact sentences, and a
    // negative assertion that spells one out would keep it alive in the
    // source forever.
    assert!(content.contains("Read this index first"));
    assert!(content.contains("the tier-2 topics your task touches"));
}

#[test]
fn test_generate_index_separates_category_sections_with_blank_lines() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    for (category, slug) in [("architecture", "merge-flow"), ("mistakes", "phantom")] {
        let dir = knowledge.root().join(category);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{slug}.md")), "# Title\n\n> Blurb.\n").unwrap();
    }

    let content = generate_index(knowledge.root()).unwrap();
    // markdownlint MD022/MD058: a heading that follows a table row directly
    // fails the pre-push markdown lint.
    let lines: Vec<&str> = content.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("###") {
            assert_eq!(
                lines[index - 1],
                "",
                "heading {line} must be preceded by a blank line"
            );
        }
    }
}

#[test]
fn test_generate_index_omits_topics_section_when_empty() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();

    let content = generate_index(knowledge.root()).unwrap();
    assert!(!content.contains("Tier 2"));
}

#[cfg(unix)]
#[test]
fn a_symlinked_topic_or_tier1_file_is_ignored_and_does_not_fail_generate_index() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    let outside = temp.path().join("outside.md");
    fs::create_dir_all(root.join("architecture")).unwrap();
    fs::write(&outside, "# Outside\n\n> Not a topic.\n").unwrap();
    fs::write(root.join("architecture/real.md"), "# Real\n\n> A topic.\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("architecture/linked.md")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("patterns.md")).unwrap();

    let content = generate_index(&root).unwrap();

    assert!(content.contains("architecture/real.md"));
    assert!(!content.contains("linked"));
    assert!(!content.contains("(patterns.md)"));
}

#[cfg(unix)]
#[test]
fn write_index_replaces_a_symlinked_index_even_when_the_target_matches() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    fs::create_dir_all(&root).unwrap();
    let target = temp.path().join("elsewhere.md");
    fs::write(&target, generate_index(&root).unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, root.join(INDEX_FILENAME)).unwrap();

    write_index(&root).unwrap();

    let metadata = fs::symlink_metadata(root.join(INDEX_FILENAME)).unwrap();
    assert!(metadata.file_type().is_file(), "the symlink is replaced");
    assert!(index_is_stale(&root).is_ok_and(|stale| !stale));
}

#[cfg(unix)]
#[test]
fn a_fifo_at_index_md_does_not_hang_write_or_stale_checks() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    fs::create_dir_all(&root).unwrap();
    nix::unistd::mkfifo(&root.join(INDEX_FILENAME), nix::sys::stat::Mode::S_IRWXU).unwrap();
    let (done, finished) = std::sync::mpsc::channel();
    let worker_root = root.clone();
    // A regression would block this thread forever; the timeout turns that
    // into a failed assertion instead of a hung test run.
    std::thread::spawn(move || {
        let stale = index_is_stale(&worker_root).is_err();
        let written = write_index(&worker_root).is_ok();
        let _ = done.send((stale, written));
    });

    let (stale_check_refused, written) = finished
        .recv_timeout(Duration::from_secs(20))
        .expect("a FIFO at INDEX.md must not block");

    assert!(stale_check_refused);
    assert!(written);
    assert!(fs::symlink_metadata(root.join(INDEX_FILENAME))
        .unwrap()
        .file_type()
        .is_file());
}
