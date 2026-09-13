use crate::fs::knowledge::catalog::{build, CatalogIssue, EvidenceUnavailableReason};
use crate::fs::knowledge::frontmatter;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_all(root: &Path, message: &str) {
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Loom Test",
            "-c",
            "user.email=loom@example.invalid",
            "commit",
            "-m",
            message,
        ],
    );
}

struct Fixture {
    _temp: TempDir,
    project: PathBuf,
    knowledge: PathBuf,
    verified: String,
}

impl Fixture {
    fn topic(&self) -> PathBuf {
        self.knowledge.join("architecture.md")
    }

    fn set_sources(&self, sources: &[&str], verified: Option<&str>) {
        let topic = self.topic();
        frontmatter::update_file(&topic, |metadata| {
            metadata.sources = sources.iter().map(|source| (*source).into()).collect();
            metadata.verified = verified.map(str::to_string);
            Ok(())
        })
        .unwrap();
    }
}

fn fixture(sources: &[&str]) -> Fixture {
    let temp = TempDir::new().unwrap();
    let project = temp.path().to_path_buf();
    let knowledge = project.join("doc/loom/knowledge");
    fs::create_dir_all(&knowledge).unwrap();
    for source in sources {
        let path = project.join(source);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "pub const VALUE: u8 = 1;\n").unwrap();
    }
    git(&project, &["init", "-q"]);
    commit_all(&project, "initial sources");
    let verified = git(&project, &["rev-parse", "HEAD"]);
    fs::write(
        knowledge.join("architecture.md"),
        "# Architecture\n\n## Topic\nBody.\n",
    )
    .unwrap();
    let fixture = Fixture {
        _temp: temp,
        project,
        knowledge,
        verified,
    };
    let sources = sources.to_vec();
    fixture.set_sources(&sources, Some(&fixture.verified));
    commit_all(&fixture.project, "record verification");
    fixture
}

fn changed_sources(catalog: &crate::fs::knowledge::catalog::Catalog) -> BTreeSet<String> {
    catalog
        .issues
        .iter()
        .filter_map(|issue| match issue {
            CatalogIssue::EvidenceChanged { source_path, .. } => Some(source_path.clone()),
            _ => None,
        })
        .collect()
}

fn unavailable_reasons(
    catalog: &crate::fs::knowledge::catalog::Catalog,
) -> Vec<EvidenceUnavailableReason> {
    catalog
        .issues
        .iter()
        .filter_map(|issue| match issue {
            CatalogIssue::EvidenceUnavailable { reason, .. } => Some(*reason),
            _ => None,
        })
        .collect()
}

#[test]
fn evidence_changed_fires_only_for_a_source_that_changed_since_verified() {
    let temp = TempDir::new().unwrap();
    let project = temp.path();
    let knowledge = project.join("doc/loom/knowledge");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(&knowledge).unwrap();
    fs::write(project.join("src/changed.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    fs::write(project.join("src/stable.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    git(project, &["init", "-q"]);
    commit_all(project, "initial sources");
    let verified = git(project, &["rev-parse", "HEAD"]);

    let topic = knowledge.join("architecture.md");
    fs::write(&topic, "# Architecture\n\n## Topic\nBody.\n").unwrap();
    frontmatter::update_file(&topic, |metadata| {
        metadata.sources = vec!["src/changed.rs".into(), "src/stable.rs".into()];
        metadata.verified = Some(verified.clone());
        Ok(())
    })
    .unwrap();
    commit_all(project, "record verification");
    fs::write(project.join("src/changed.rs"), "pub const VALUE: u8 = 2;\n").unwrap();
    commit_all(project, "change one source");

    let catalog = build(&knowledge).unwrap();

    let changed: Vec<_> = catalog
        .issues
        .iter()
        .filter_map(|issue| match issue {
            CatalogIssue::EvidenceChanged { source_path, .. } => Some(source_path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(changed, vec!["src/changed.rs"]);
}

#[test]
fn evidence_unchanged_verified_source_is_current() {
    let fixture = fixture(&["src/stable.rs"]);

    let catalog = build(&fixture.knowledge).unwrap();
    let summary =
        crate::fs::knowledge::catalog::evidence_summary(&fixture.knowledge, &catalog.issues);

    assert!(changed_sources(&catalog).is_empty());
    assert_eq!(summary.status, "current");
    assert_eq!(summary.current, 1);
}

#[test]
fn evidence_reports_staged_and_unstaged_source_changes() {
    let fixture = fixture(&["src/staged.rs", "src/unstaged.rs"]);
    fs::write(
        fixture.project.join("src/staged.rs"),
        "pub const VALUE: u8 = 2;\n",
    )
    .unwrap();
    git(&fixture.project, &["add", "src/staged.rs"]);
    fs::write(
        fixture.project.join("src/unstaged.rs"),
        "pub const VALUE: u8 = 3;\n",
    )
    .unwrap();

    let catalog = build(&fixture.knowledge).unwrap();

    assert_eq!(
        changed_sources(&catalog),
        BTreeSet::from(["src/staged.rs".to_string(), "src/unstaged.rs".to_string(),])
    );
}

#[test]
fn evidence_reports_declared_untracked_source() {
    let fixture = fixture(&["src/stable.rs"]);
    fs::write(
        fixture.project.join("src/untracked.rs"),
        "pub const VALUE: u8 = 2;\n",
    )
    .unwrap();
    fixture.set_sources(
        &["src/stable.rs", "src/untracked.rs"],
        Some(&fixture.verified),
    );

    let catalog = build(&fixture.knowledge).unwrap();

    assert_eq!(
        changed_sources(&catalog),
        BTreeSet::from(["src/untracked.rs".to_string()])
    );
}

#[test]
fn evidence_reports_deleted_and_renamed_declared_sources() {
    let fixture = fixture(&["src/deleted.rs", "src/renamed.rs"]);
    fs::remove_file(fixture.project.join("src/deleted.rs")).unwrap();
    git(
        &fixture.project,
        &[
            "--literal-pathspecs",
            "mv",
            "src/renamed.rs",
            "src/new-name.rs",
        ],
    );

    let catalog = build(&fixture.knowledge).unwrap();

    assert_eq!(
        changed_sources(&catalog),
        BTreeSet::from(["src/deleted.rs".to_string(), "src/renamed.rs".to_string(),])
    );
}

#[test]
fn evidence_handles_literal_space_newline_and_pathspec_sources() {
    let sources = ["src/space name\nfile.rs", ":(literal)*.rs"];
    let fixture = fixture(&sources);
    for source in sources {
        fs::write(fixture.project.join(source), "pub const VALUE: u8 = 2;\n").unwrap();
        git(&fixture.project, &["--literal-pathspecs", "add", source]);
    }

    let catalog = build(&fixture.knowledge).unwrap();

    assert_eq!(
        changed_sources(&catalog),
        BTreeSet::from([
            "src/space name\nfile.rs".to_string(),
            ":(literal)*.rs".to_string(),
        ])
    );
}

#[test]
fn evidence_reports_missing_and_invalid_revisions_as_unavailable() {
    let fixture = fixture(&["src/stable.rs"]);
    fixture.set_sources(&["src/stable.rs"], None);

    let missing = build(&fixture.knowledge).unwrap();
    assert_eq!(
        unavailable_reasons(&missing),
        vec![EvidenceUnavailableReason::MissingRevision]
    );

    fixture.set_sources(&["src/stable.rs"], Some("not-a-commit"));
    let invalid = build(&fixture.knowledge).unwrap();
    assert_eq!(
        unavailable_reasons(&invalid),
        vec![EvidenceUnavailableReason::InvalidRevision]
    );
}

#[test]
fn evidence_reports_missing_repository_as_unavailable() {
    let temp = TempDir::new().unwrap();
    let project = temp.path();
    let knowledge = project.join("doc/loom/knowledge");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::create_dir_all(&knowledge).unwrap();
    fs::write(project.join("src/stable.rs"), "pub const VALUE: u8 = 1;\n").unwrap();
    fs::write(
        knowledge.join("topic.md"),
        "---\nsources:\n  - src/stable.rs\nverified: abcdef\n---\n# Topic\n",
    )
    .unwrap();

    let catalog = build(&knowledge).unwrap();

    assert_eq!(
        unavailable_reasons(&catalog),
        vec![EvidenceUnavailableReason::MissingRepository]
    );
}

#[test]
fn strict_ignores_review_only_issues() {
    let issues = [
        CatalogIssue::EvidenceChanged {
            file: PathBuf::from("architecture.md"),
            source_path: "src/a.rs".into(),
            verified: "0123456789abcdef".into(),
        },
        CatalogIssue::UnverifiableReference {
            file: PathBuf::from("architecture.md"),
            source_path: "foo.rs".into(),
            kind: "example".into(),
        },
        CatalogIssue::EvidenceUnavailable {
            file: PathBuf::from("architecture.md"),
            source_path: "src/a.rs".into(),
            reason: EvidenceUnavailableReason::GitUnavailable,
        },
    ];

    assert_eq!(
        issues
            .iter()
            .filter(|issue| !issue.is_review_only())
            .count(),
        0
    );
}
