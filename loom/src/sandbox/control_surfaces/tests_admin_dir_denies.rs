//! The administrative directory denies of a session, by its relation to each
//! directory: its own keeps only `config.worktree` denied, every other one is
//! denied whole, nothing nests, and the common directory, `refs/` and the
//! object store stay writable.

use super::tests::{has, repo_with_two_worktrees};
use super::*;

/// The denies of a session at `worktree` (`None`: in the checkout) of `repo`.
fn denies_at(repo: &Path, worktree: Option<&Path>) -> Result<SessionDenies> {
    session_denies(&DenyInputs {
        repo_root: repo,
        state_root: &repo.join(".loom").join("work"),
        worktree,
        executable_dirs: &[],
        plugin_entries: None,
        writable_roots: &[],
    })
}

/// The canonical git common directory of `repo`.
fn common(repo: &Path) -> PathBuf {
    repo.join(".git").canonicalize().unwrap()
}

/// Both session placements: in the stage worktree `s1`, and in the checkout.
fn placements(repo: &Path) -> [Option<PathBuf>; 2] {
    [Some(repo.join(".worktrees").join("s1")), None]
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// `dir` is denied whole in both layers.
fn assert_denied_whole(denies: &SessionDenies, dir: &Path) {
    assert!(has(&denies.deny_write, text(dir)), "{dir:?}: {denies:?}");
    let rule = format!("Edit(/{}/**)", text(dir));
    assert!(has(&denies.edit, &rule), "{rule}: {denies:?}");
}

/// No entry of either layer lies strictly beneath `dir`.
fn assert_nothing_beneath(denies: &SessionDenies, dir: &Path) {
    for entry in &denies.deny_write {
        let beneath = Path::new(entry)
            .strip_prefix(dir)
            .is_ok_and(|rest| !rest.as_os_str().is_empty());
        assert!(!beneath, "{entry} lies beneath {dir:?}: {denies:?}");
    }
    let whole = format!("Edit(/{}/**)", text(dir));
    let prefix = format!("Edit(/{}/", text(dir));
    for rule in &denies.edit {
        let beneath = rule != &whole && rule.starts_with(&prefix);
        assert!(!beneath, "{rule} lies beneath {dir:?}: {denies:?}");
    }
}

#[test]
fn each_admin_dir_is_denied_by_its_relation_to_the_session() {
    let (_tmp, repo) = repo_with_two_worktrees();
    let registry = common(&repo).join("worktrees");
    let (s1, s2) = (registry.join("s1"), registry.join("s2"));

    let stage = denies_at(&repo, Some(&repo.join(".worktrees").join("s1"))).unwrap();
    let own_config = s1.join("config.worktree");
    assert!(has(&stage.deny_write, text(&own_config)), "{stage:?}");
    let rule = format!("Edit(/{})", text(&own_config));
    assert!(has(&stage.edit, &rule), "{stage:?}");
    assert!(!has(&stage.deny_write, text(&s1)), "{stage:?}");
    assert!(
        !has(&stage.edit, &format!("Edit(/{}/**)", text(&s1))),
        "{stage:?}"
    );
    assert!(
        !has(&stage.edit, &format!("Edit(/{})", text(&s1))),
        "{stage:?}"
    );
    assert_denied_whole(&stage, &s2);
    assert_nothing_beneath(&stage, &s2);

    let checkout = denies_at(&repo, None).unwrap();
    assert_denied_whole(&checkout, &s1);
    assert_denied_whole(&checkout, &s2);
    let configs = |entry: &String| entry.contains("config.worktree");
    assert!(!checkout.deny_write.iter().any(configs), "{checkout:?}");
    assert!(!checkout.edit.iter().any(configs), "{checkout:?}");
}

#[test]
fn no_admin_dir_deny_lies_beneath_another_deny() {
    let (_tmp, repo) = repo_with_two_worktrees();
    let registry = common(&repo).join("worktrees");
    for worktree in placements(&repo) {
        let denies = denies_at(&repo, worktree.as_deref()).unwrap();
        for outer in &denies.deny_write {
            for inner in &denies.deny_write {
                let (outer, inner) = (Path::new(outer), Path::new(inner));
                let nested = inner != outer && inner.starts_with(outer);
                let admin = outer.starts_with(&registry) || inner.starts_with(&registry);
                assert!(
                    !(nested && admin),
                    "{inner:?} lies beneath {outer:?} for {worktree:?}: {denies:?}"
                );
            }
        }
    }
}

#[test]
fn the_refs_the_objects_and_the_common_dir_stay_writable() {
    let (_tmp, repo) = repo_with_two_worktrees();
    let common = common(&repo);
    let kept = [common.clone(), common.join("refs"), common.join("objects")];
    for worktree in placements(&repo) {
        let denies = denies_at(&repo, worktree.as_deref()).unwrap();
        for entry in &denies.deny_write {
            let entry = Path::new(entry);
            for path in &kept {
                assert!(
                    !path.starts_with(entry),
                    "{entry:?} denies {path:?} for {worktree:?}: {denies:?}"
                );
            }
        }
    }
}

#[test]
fn an_unregistered_worktree_refuses_the_capsule_denies() {
    let (_tmp, repo) = repo_with_two_worktrees();
    let ghost = repo.join(".worktrees").join("ghost");
    std::fs::create_dir_all(&ghost).unwrap();

    let error = denies_at(&repo, Some(&ghost)).unwrap_err();

    assert!(format!("{error:#}").contains("ghost"), "{error:#}");
}
