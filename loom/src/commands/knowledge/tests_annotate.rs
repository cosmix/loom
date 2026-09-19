use super::annotate::{annotate_path, parse_state, Annotation};
use crate::context::schema::LifecycleState;
use crate::fs::knowledge::frontmatter;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn annotate_writes_frontmatter_without_touching_the_body() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topic.md");
    let body = "# Topic\n\n## Details\nThe body stays byte-identical.\n";
    fs::write(
        &path,
        format!("---\nowner: docs\nsources: [src/old.rs]\n---\n{body}"),
    )
    .unwrap();
    let annotation = Annotation {
        state: Some(LifecycleState::Draft),
        sources: vec!["loom/src/context/pack.rs".into()],
        clear_sources: true,
        aliases: vec!["packing".into()],
        ..Default::default()
    };

    annotate_path(&path, &annotation).unwrap();

    let updated = fs::read_to_string(&path).unwrap();
    assert!(updated.contains("owner: docs\n"));
    assert!(updated.ends_with(body));
    let metadata = frontmatter::file_frontmatter(updated.as_bytes());
    assert_eq!(metadata.state, Some(LifecycleState::Draft));
    assert_eq!(metadata.sources, vec!["loom/src/context/pack.rs"]);
    assert_eq!(metadata.aliases, vec!["packing"]);
}

#[test]
fn annotate_verified_head_resolves_the_full_revision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let path = root.join("topic.md");
    fs::write(&path, "# Topic\n\nBody.\n").unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "topic.md"]);
    git(
        root,
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@example.invalid",
            "commit",
            "-m",
            "initial",
        ],
    );
    let head = git(root, &["rev-parse", "HEAD"]);
    let annotation = Annotation {
        verified: Some(super::annotate::resolve_revision(root, "HEAD").unwrap()),
        ..Default::default()
    };

    annotate_path(&path, &annotation).unwrap();

    let updated = fs::read(&path).unwrap();
    assert_eq!(frontmatter::file_frontmatter(&updated).verified, Some(head));
}

#[test]
fn annotate_with_only_a_blurb_does_not_introduce_a_frontmatter_block() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topic.md");
    fs::write(
        &path,
        "# Topic\n\n> Old description.\n\n## Details\nBody.\n",
    )
    .unwrap();
    let annotation = Annotation {
        blurb: Some("Concise current description.".into()),
        ..Default::default()
    };

    annotate_path(&path, &annotation).unwrap();

    let updated = fs::read_to_string(&path).unwrap();
    assert!(!updated.starts_with("---"));
    assert_eq!(updated.lines().next(), Some("# Topic"));
}

#[test]
fn annotate_rejects_an_unknown_state() {
    let error = parse_state("forgotten").unwrap_err();

    assert!(error
        .to_string()
        .contains("Unknown knowledge state 'forgotten'"));
}

#[test]
fn annotate_blurb_rewrites_the_index_description_line() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("topic.md");
    fs::write(
        &path,
        "# Topic\n\n> Old description.\n\n## Details\nBody.\n",
    )
    .unwrap();
    let annotation = Annotation {
        blurb: Some("Concise current description.".into()),
        ..Default::default()
    };

    annotate_path(&path, &annotation).unwrap();

    let updated = fs::read_to_string(path).unwrap();
    assert!(updated.contains("\n> Concise current description.\n"));
    assert!(!updated.contains("Old description."));
}

fn topic_with(
    content: &str,
) -> (
    tempfile::TempDir,
    crate::fs::knowledge::KnowledgeDir,
    crate::fs::knowledge::KnowledgeTarget,
) {
    let temp = tempfile::tempdir().unwrap();
    let knowledge = crate::fs::knowledge::KnowledgeDir::from_root(temp.path());
    let target = crate::fs::knowledge::KnowledgeTarget::parse("architecture/topic").unwrap();
    let path = knowledge.target_path(&target);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, content).unwrap();
    (temp, knowledge, target)
}

#[test]
fn section_state_writes_then_replaces_the_marker_under_the_heading() {
    let (_temp, knowledge, target) =
        topic_with("# T\n\n## Old episode\n\nbody\n\n## Current\n\nnow\n");

    knowledge
        .set_section_state_target(&target, "Old episode", LifecycleState::Deprecated)
        .unwrap();
    knowledge
        .set_section_state_target(&target, "Old episode", LifecycleState::Historical)
        .unwrap();

    let text = fs::read_to_string(knowledge.target_path(&target)).unwrap();
    assert_eq!(
        text,
        "# T\n\n## Old episode\n<!-- state: historical -->\n\nbody\n\n## Current\n\nnow\n"
    );
}

#[test]
fn section_state_refuses_a_missing_or_nested_heading() {
    let original = "# T\n\n## Group\n\n### Nested\n\nx\n";
    let (_temp, knowledge, target) = topic_with(original);

    assert!(knowledge
        .set_section_state_target(&target, "Absent", LifecycleState::Historical)
        .is_err());
    let nested = knowledge
        .set_section_state_target(&target, "Nested", LifecycleState::Historical)
        .unwrap_err();
    assert!(nested.to_string().contains("level-3"), "{nested}");
    assert_eq!(
        fs::read_to_string(knowledge.target_path(&target)).unwrap(),
        original
    );
}

#[test]
fn a_section_marker_overrides_the_file_state_and_is_not_part_of_the_body() {
    let text = "---\nstate: active\n---\n# T\n\n## Old episode\n<!-- state: historical -->\n\nbody\n\n## Current\n\nnow\n";

    let chunks =
        crate::fs::knowledge::chunker::chunk_file(Path::new("architecture/t.md"), text.as_bytes())
            .unwrap();

    let old = chunks
        .iter()
        .find(|chunk| chunk.heading == "Old episode")
        .unwrap();
    assert_eq!(old.state, LifecycleState::Historical);
    assert!(!old.body.contains("<!--"), "body: {:?}", old.body);
    let current = chunks
        .iter()
        .find(|chunk| chunk.heading == "Current")
        .unwrap();
    assert_eq!(current.state, LifecycleState::Active);
}

#[test]
fn a_historical_section_is_retrieved_only_under_the_historical_policy() {
    use crate::context::retrieve::{retrieve_for_stage, StageQuery};
    use crate::context::schema::LifecyclePolicy;
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    let topic = temp.path().join("doc/loom/knowledge/architecture/topic.md");
    fs::create_dir_all(topic.parent().unwrap()).unwrap();
    fs::write(
        &topic,
        "# Topic\n\n## Current notes\n\nunrelated present wording\n\n\
         ## Old episode\n<!-- state: historical -->\n\nquokka-zebra archival wording\n",
    )
    .unwrap();
    let is_old = |state: LifecycleState| state == LifecycleState::Historical;

    let current = retrieve_for_stage(
        &StageQuery::new(temp.path(), "quokka zebra archival"),
        2_000,
    )
    .unwrap();
    let mut query = StageQuery::new(temp.path(), "quokka zebra archival");
    query.lifecycle = LifecyclePolicy::Historical;
    let historical = retrieve_for_stage(&query, 2_000).unwrap();

    assert!(!current.items.iter().any(|item| is_old(item.state)));
    assert!(historical.items.iter().any(|item| is_old(item.state)));
}
