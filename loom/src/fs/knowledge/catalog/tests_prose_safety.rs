use super::sources_for_knowledge_root;
use crate::context::fingerprint::fingerprint_tree;
use crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES;
use crate::fs::knowledge::catalog;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use tempfile::TempDir;

fn setup(root: &Path) -> std::path::PathBuf {
    let knowledge = root.join("doc/loom/knowledge");
    fs::create_dir_all(&knowledge).unwrap();
    fs::write(knowledge.join("architecture.md"), "# Curated\n\nBody.\n").unwrap();
    knowledge
}

fn assert_no_prose(knowledge: &Path) {
    let catalog = catalog::build(knowledge).unwrap();
    assert!(catalog
        .chunks
        .iter()
        .all(|chunk| !chunk.id.starts_with("prose:")));
    let fingerprints = fingerprint_tree(knowledge).unwrap();
    assert!(fingerprints
        .iter()
        .all(|fp| !fp.path.to_string_lossy().starts_with("prose:")));
}

#[test]
fn configured_root_symlink_and_intermediate_symlink_are_excluded() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(
        outside.path().join("external.md"),
        "# External\n\nSecret.\n",
    )
    .unwrap();
    let root = temp.path();
    let knowledge = setup(root);
    symlink(outside.path(), root.join("docs-link")).unwrap();
    fs::create_dir(root.join("subdir")).unwrap();
    symlink(outside.path(), root.join("subdir/docs-link")).unwrap();
    fs::create_dir(root.join(".loom")).unwrap();
    for configured in ["docs-link", "subdir/docs-link"] {
        fs::write(
            root.join(".loom/config.toml"),
            format!("[retrieval]\nprose_roots = [\"{configured}\"]\n"),
        )
        .unwrap();
        assert_no_prose(&knowledge);
    }
}

#[test]
fn descendant_symlink_is_excluded_but_nested_roots_deduplicate() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(
        outside.path().join("external.md"),
        "# External\n\nSecret.\n",
    )
    .unwrap();
    let root = temp.path();
    let knowledge = setup(root);
    fs::create_dir_all(root.join("doc/nested")).unwrap();
    fs::write(root.join("doc/nested/valid.md"), "# Valid\n\nBody.\n").unwrap();
    symlink(outside.path(), root.join("doc/nested/link")).unwrap();
    fs::create_dir(root.join(".loom")).unwrap();
    fs::write(
        root.join(".loom/config.toml"),
        "[retrieval]\nprose_roots = [\"doc\", \"doc/nested\"]\n",
    )
    .unwrap();
    let sources = sources_for_knowledge_root(&knowledge).unwrap();
    let mut files = Vec::new();
    sources.for_each_file(|path, bytes| files.push((path, bytes)));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, Path::new("doc/nested/valid.md"));
    let fingerprints = fingerprint_tree(&knowledge).unwrap();
    assert_eq!(
        fingerprints
            .iter()
            .filter(|fp| fp.path.to_string_lossy().starts_with("prose:"))
            .count(),
        1
    );
}

#[test]
fn linked_worktree_uses_main_project_prose_configuration() {
    let temp = TempDir::new().unwrap();
    let main = temp.path().join("main");
    let stage = temp.path().join("stage");
    fs::create_dir_all(main.join(".git")).unwrap();
    fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::create_dir_all(main.join(".loom/work")).unwrap();
    fs::write(main.join(".loom/work/config.toml"), "[plan]\n").unwrap();
    fs::write(
        main.join(".loom/config.toml"),
        "[retrieval]\nprose_roots = []\n",
    )
    .unwrap();
    let knowledge = setup(&stage);
    fs::write(stage.join("doc/design.md"), "# Design\n\nBody.\n").unwrap();
    fs::create_dir_all(stage.join(".loom")).unwrap();
    fs::write(
        stage.join(".loom/config.toml"),
        "[retrieval]\nprose_roots = [\"doc\"]\n",
    )
    .unwrap();
    symlink(main.join(".loom/work"), stage.join(".loom/work")).unwrap();

    assert_no_prose(&knowledge);
}

#[test]
fn replacement_after_discovery_cannot_escape_or_exceed_cap() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let root = temp.path();
    let knowledge = setup(root);
    let candidate = root.join("doc/candidate.md");
    fs::write(&candidate, "# Candidate\n\nBody.\n").unwrap();
    let sources = sources_for_knowledge_root(&knowledge).unwrap();
    assert!(sources
        .files()
        .iter()
        .any(|path| path == Path::new("doc/candidate.md")));

    let outside_file = outside.path().join("external.md");
    fs::write(&outside_file, "# External\n\nSecret.\n").unwrap();
    fs::remove_file(&candidate).unwrap();
    symlink(&outside_file, &candidate).unwrap();
    sources.for_each_file(|_, _| panic!("symlink reached prose reader"));
    assert_no_prose(&knowledge);

    fs::remove_file(&candidate).unwrap();
    fs::write(&candidate, vec![b'#'; MAX_EXTRACTED_FILE_BYTES + 1]).unwrap();
    sources.for_each_file(|_, _| panic!("oversized file reached prose reader"));
    assert_no_prose(&knowledge);

    fs::remove_file(&candidate).unwrap();
    nix::unistd::mkfifo(&candidate, nix::sys::stat::Mode::S_IRWXU).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        sources.for_each_file(|_, _| panic!("FIFO reached prose reader"));
        sender.send(()).unwrap();
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("FIFO read blocked");
    assert_no_prose(&knowledge);
}
