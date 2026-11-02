use super::*;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::plan_server::context::BindInfo;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::planning::watch::OwnWrites;
use tempfile::TempDir;

#[path = "environment_tests.rs"]
mod environment;
#[path = "tests_route.rs"]
mod route;
#[path = "shutdown_tests.rs"]
mod shutdown;

struct Repo(TempDir);

impl Repo {
    fn new() -> Self {
        let repo = Self(tempfile::tempdir().unwrap());
        repo.git(&["init", "-q"]);
        let hooks = repo.0.path().join("hooks");
        fs::create_dir(&hooks).unwrap();
        for (key, value) in [
            ("user.name", "Commit Tests"),
            ("user.email", "commit@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.hooksPath", hooks.to_str().unwrap()),
        ] {
            repo.git(&["config", key, value]);
        }
        repo.put("doc/a.md", "original\n");
        repo.put("src.txt", "original\n");
        repo.git(&["add", "--", "doc/a.md", "src.txt"]);
        repo.git(&["commit", "-q", "-m", "initial"]);
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.0.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn put(&self, path: &str, text: &str) {
        let path = self.0.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn hook(&self, name: &str, script: &str) {
        let path = self.0.path().join("hooks").join(name);
        fs::write(&path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn commit(&self, paths: &[&str], message: &str) -> Result<i32> {
        let (current, _) = super::super::changes::split_changes(self.0.path()).unwrap();
        commit_doc_changes(
            self.0.path(),
            &strings(paths),
            message,
            &current,
            &mut |_| {},
        )
    }

    fn context(&self, read_only: bool) -> Arc<PlanContext> {
        let problems = if read_only {
            vec![MissingPrereq {
                what: PrereqKind::Tmux,
                message: "missing tmux".into(),
                fix: "install tmux".into(),
            }]
        } else {
            Vec::new()
        };
        let ctx = Arc::new(
            PlanContext::new(
                self.0.path().to_owned(),
                ReadOnly::from_problems(problems),
                BindInfo::new("127.0.0.1:7272".parse().unwrap(), "token"),
                OwnWrites::default(),
            )
            .unwrap(),
        );
        child::install_jobs(&ctx);
        install(&ctx);
        ctx
    }
}

fn strings(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_owned()).collect()
}

#[test]
#[serial_test::serial]
fn refuses_unselected_or_unsafe_paths_before_staging() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let head = repo.git(&["rev-parse", "HEAD"]);
    for paths in [
        vec![],
        vec!["src.txt"],
        vec!["doc/missing.md"],
        vec!["doc/a.md", "src.txt"],
        vec!["../a"],
        vec!["/doc/a.md"],
    ] {
        assert!(repo.commit(&paths, "edit").is_err(), "{paths:?}");
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "");
    }
}

#[test]
fn rejects_traversal_even_if_the_current_list_contains_it() {
    for path in [
        "doc/../src.txt",
        "doc/x/../../src.txt",
        "/doc/a.md",
        "doc/.",
        "doc/a..md",
    ] {
        let current = [ChangedPath {
            path: path.into(),
            status: " M".into(),
        }];
        let result = commit_doc_changes(
            Path::new("/nonexistent"),
            &strings(&[path]),
            "edit",
            &current,
            &mut |_| panic!("validation must precede commands"),
        );
        assert!(result.is_err(), "{path}");
    }
}

#[test]
#[serial_test::serial]
fn refuses_empty_oversized_and_nul_messages_before_staging() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let nul = "edit\0more".to_owned();
    for message in [String::new(), "x".repeat(2_001), "é".repeat(1_001), nul] {
        assert!(repo.commit(&["doc/a.md"], &message).is_err());
        assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
        assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "");
    }
}

#[test]
#[serial_test::serial]
fn accepts_a_message_at_the_byte_limit() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    assert_eq!(repo.commit(&["doc/a.md"], &"é".repeat(1_000)).unwrap(), 0);
}

#[test]
#[serial_test::serial]
fn failing_pre_commit_streams_both_streams_and_stops_later_hooks() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    repo.hook(
        "pre-commit",
        "#!/bin/sh\necho hook-out\necho hook-err >&2\nexit 1\n",
    );
    repo.hook("commit-msg", "#!/bin/sh\ntouch later-hook\n");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let (current, _) = super::super::changes::split_changes(repo.0.path()).unwrap();
    let mut lines = Vec::new();

    let code = commit_doc_changes(
        repo.0.path(),
        &strings(&["doc/a.md"]),
        "edit",
        &current,
        &mut |line| lines.push(line.to_owned()),
    )
    .unwrap();

    assert_eq!(code, 1);
    assert!(lines.contains(&"hook-out".to_owned()));
    assert!(lines.contains(&"hook-err".to_owned()));
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
    assert!(!repo.0.path().join("later-hook").exists());
}

#[test]
#[serial_test::serial]
fn signal_death_returns_minus_one_without_moving_head() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    repo.hook("pre-commit", "#!/bin/sh\nkill -TERM \"$PPID\"\n");
    let head = repo.git(&["rev-parse", "HEAD"]);

    assert_eq!(repo.commit(&["doc/a.md"], "edit").unwrap(), -1);

    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}

#[test]
#[serial_test::serial]
fn failed_add_never_runs_commit() {
    let repo = Repo::new();
    repo.put("doc/a.md", "changed\n");
    repo.hook("pre-commit", "#!/bin/sh\ntouch hook-ran\n");
    repo.put(".git/index.lock", "locked");
    let head = repo.git(&["rev-parse", "HEAD"]);
    let mut lines = Vec::new();

    let code = run_sequence(
        repo.0.path(),
        &strings(&["doc/a.md"]),
        "edit",
        |step, stream, line| lines.push((step.to_owned(), stream, line.to_owned())),
    )
    .unwrap();

    assert_ne!(code, 0);
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|(step, _, _)| step == "git add"));
    assert!(!repo.0.path().join("hook-ran").exists());
    assert_eq!(repo.git(&["rev-parse", "HEAD"]), head);
}

#[test]
#[serial_test::serial]
fn commits_only_ticked_paths_and_preserves_unrelated_staging() {
    let repo = Repo::new();
    repo.put("src.txt", "changed\n");
    repo.git(&["add", "--", "src.txt"]);
    repo.put("doc/a.md", "changed\n");
    repo.put("doc/unticked.md", "unticked\n");
    repo.git(&["add", "--", "doc/unticked.md"]);

    assert_eq!(repo.commit(&["doc/a.md"], "edit").unwrap(), 0);

    assert_eq!(
        repo.git(&["show", "--format=", "--name-only", "HEAD"]),
        "doc/a.md\n"
    );
    assert_eq!(
        repo.git(&["diff", "--cached", "--name-only"]),
        "doc/unticked.md\nsrc.txt\n"
    );
    assert_eq!(repo.git(&["show", "HEAD:src.txt"]), "original\n");
}

#[test]
#[serial_test::serial]
fn spaces_and_glob_characters_are_literal() {
    let repo = Repo::new();
    repo.put("doc/ab.md", "unticked\n");
    for path in ["doc/a*", "doc/a?.md", "doc/[ab].md", "doc/a b.md"] {
        repo.put(path, "selected\n");
        assert_eq!(repo.commit(&[path], "add literal path").unwrap(), 0);
        assert_eq!(
            repo.git(&["show", "--format=", "--name-only", "HEAD"]),
            format!("{path}\n")
        );
        assert_eq!(repo.git(&["ls-files", "--", "doc/ab.md"]), "");
    }
}

fn head_files(repo: &Repo) -> String {
    repo.git(&["ls-tree", "-r", "--name-only", "HEAD"])
}

#[test]
#[serial_test::serial]
fn commits_a_staged_rename_listing_its_source_and_destination() {
    let repo = Repo::new();
    repo.git(&["mv", "doc/a.md", "doc/b.md"]);
    let (current, _) = super::super::changes::split_changes(repo.0.path()).unwrap();
    let listed: Vec<_> = current.iter().map(|change| change.path.as_str()).collect();
    assert_eq!(listed, ["doc/b.md", "doc/a.md"]);

    assert_eq!(repo.commit(&listed, "move a to b").unwrap(), 0);

    assert_eq!(head_files(&repo), "doc/b.md\nsrc.txt\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}

#[test]
#[serial_test::serial]
fn commits_a_staged_deletion() {
    let repo = Repo::new();
    repo.git(&["rm", "-q", "doc/a.md"]);

    assert_eq!(repo.commit(&["doc/a.md"], "drop a").unwrap(), 0);

    assert_eq!(head_files(&repo), "src.txt\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}

#[test]
#[serial_test::serial]
fn commits_an_unstaged_deletion() {
    let repo = Repo::new();
    fs::remove_file(repo.0.path().join("doc/a.md")).unwrap();

    assert_eq!(repo.commit(&["doc/a.md"], "drop a").unwrap(), 0);

    assert_eq!(head_files(&repo), "src.txt\n");
    assert_eq!(repo.git(&["status", "--porcelain"]), "");
}

#[test]
#[serial_test::serial]
fn commits_deletions_beside_edits_and_keeps_unrelated_staging() {
    let repo = Repo::new();
    repo.put("doc/c.md", "original\n");
    repo.git(&["add", "--", "doc/c.md"]);
    repo.git(&["commit", "-q", "-m", "add c"]);
    repo.git(&["rm", "-q", "doc/a.md"]);
    repo.put("doc/c.md", "changed\n");
    repo.put("doc/new.md", "new\n");
    repo.put("src.txt", "changed\n");
    repo.git(&["add", "--", "src.txt"]);

    let paths = ["doc/a.md", "doc/c.md", "doc/new.md"];

    assert_eq!(repo.commit(&paths, "docs").unwrap(), 0);

    assert_eq!(head_files(&repo), "doc/c.md\ndoc/new.md\nsrc.txt\n");
    assert_eq!(repo.git(&["show", "HEAD:doc/c.md"]), "changed\n");
    assert_eq!(repo.git(&["show", "HEAD:src.txt"]), "original\n");
    assert_eq!(repo.git(&["diff", "--cached", "--name-only"]), "src.txt\n");
}
