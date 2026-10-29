use super::*;
use std::fs;
use std::os::unix::fs::symlink;
use tempfile::{tempdir, TempDir};

fn fixture() -> Result<(TempDir, SessionDir)> {
    let root = tempdir()?;
    let session = root.path().join("session");
    fs::create_dir(&session)?;
    Ok((root, SessionDir::new(session)))
}

fn put(root: &Path, relative: &str) -> Result<()> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, "existing")?;
    Ok(())
}

fn start(root: &Path, dir: &SessionDir) -> Result<()> {
    write(dir, &SessionScope::kickoff(root)?)
}

fn assert_denied(scope: &SessionScope, path: &str, expected: &str) {
    let reason = scope.allows_write(Path::new(path)).unwrap_err();
    assert!(reason.contains(expected), "{reason}");
}

#[test]
fn open_allows_everything_without_peer_tools() {
    for path in [
        "doc/product/new/index.md",
        "doc/loom/config.md",
        "src/main.rs",
        "/tmp/x",
    ] {
        assert_eq!(SessionScope::Open.allows_write(Path::new(path)), Ok(()));
    }
    assert!(!SessionScope::Open.peer_tools());
}

#[test]
fn kickoff_denies_documents_until_confirmation() -> Result<()> {
    let root = tempdir()?;
    let scope = SessionScope::kickoff(root.path())?;
    for path in [
        "doc/product/new/index.md",
        "doc/plans/PLAN-new.md",
        "doc/other.md",
        "doc/plans/briefs/new/brief.md",
    ] {
        assert_denied(&scope, path, "until the size and name are confirmed");
    }
    assert!(!scope.peer_tools());
    Ok(())
}

#[test]
fn feature_grant_allows_only_its_index() -> Result<()> {
    let (root, dir) = fixture()?;
    start(root.path(), &dir)?;
    let armed = arm_kickoff(&dir, root.path(), KickoffSize::Feature, "new-feature")?;
    assert_eq!(
        armed.document,
        Path::new("doc/product/new-feature/index.md")
    );
    assert_eq!(armed.bundle, None);
    let scope = load(&dir)?;
    assert_eq!(scope.allows_write(&armed.document), Ok(()));
    for path in [
        "doc/product/new-feature/section_1.md",
        "doc/product/other/index.md",
        "doc/product/new-feature",
        "doc/plans/PLAN-new-feature.md",
    ] {
        assert_denied(&scope, path, "only doc/product/new-feature/index.md");
    }
    Ok(())
}

#[test]
fn plan_grant_allows_its_document_and_strict_bundle_descendants() -> Result<()> {
    let (root, dir) = fixture()?;
    start(root.path(), &dir)?;
    let armed = arm_kickoff(&dir, root.path(), KickoffSize::Plan, "new-plan")?;
    assert_eq!(armed.document, Path::new("doc/plans/PLAN-new-plan.md"));
    assert_eq!(armed.bundle, Some("doc/plans/briefs/new-plan".into()));
    let scope = load(&dir)?;
    for path in [
        "doc/plans/PLAN-new-plan.md",
        "doc/plans/briefs/new-plan/brief.md",
        "doc/plans/briefs/new-plan/nested/unit.md",
    ] {
        assert_eq!(scope.allows_write(Path::new(path)), Ok(()));
    }
    for path in [
        "doc/plans/briefs/new-plan",
        "doc/plans/briefs/new-plan-sibling/x.md",
        "doc/plans/PLAN-other.md",
        "doc/product/new-plan/index.md",
    ] {
        assert_denied(&scope, path, "only doc/plans/PLAN-new-plan.md");
    }
    Ok(())
}

#[test]
fn arm_refuses_invalid_names() -> Result<()> {
    let (root, dir) = fixture()?;
    start(root.path(), &dir)?;
    for name in ["Uppercase", "with/slash", "..", "", "archive", "briefs"] {
        for size in [KickoffSize::Feature, KickoffSize::Plan] {
            let result = arm_kickoff(&dir, root.path(), size, name);
            assert_eq!(result, Err(ArmRefusal::InvalidName(name.into())));
        }
    }
    Ok(())
}

fn assert_clash(size: KickoffSize, name: &str, file: &str, clash: &str) -> Result<()> {
    for present_at_start in [true, false] {
        let (root, dir) = fixture()?;
        if present_at_start {
            put(root.path(), file)?;
        }
        start(root.path(), &dir)?;
        if present_at_start {
            fs::remove_dir_all(root.path().join("doc"))?;
        } else {
            put(root.path(), file)?;
        }
        let result = arm_kickoff(&dir, root.path(), size, name);
        assert_eq!(result, Err(ArmRefusal::Exists(clash.into())));
    }
    Ok(())
}

#[test]
fn arm_refuses_feature_names_case_insensitively_from_snapshot_and_live_listing() -> Result<()> {
    for name in ["foo", "Foo"] {
        let directory = format!("doc/product/{name}");
        let file = format!("{directory}/index.md");
        assert_clash(KickoffSize::Feature, "foo", &file, &directory)?;
    }
    Ok(())
}

#[test]
fn arm_refuses_features_known_only_through_their_plan_directory() -> Result<()> {
    for name in ["foo", "Foo"] {
        let directory = format!("doc/plans/{name}");
        let file = format!("{directory}/index.md");
        assert_clash(KickoffSize::Feature, "foo", &file, &directory)?;
    }
    Ok(())
}

#[test]
fn arm_refuses_plan_slug_clashes_in_every_home_and_archive() -> Result<()> {
    for path in [
        "doc/plans/PLAN-Foo.md",
        "doc/plans/feature/IN_PROGRESS-PLAN-Foo.md",
        "doc/plans/archive/DONE-PLAN-Foo.md",
        "doc/plans/feature/archive/RETIRED-PLAN-Foo.md",
    ] {
        assert_clash(KickoffSize::Plan, "foo", path, path)?;
    }
    Ok(())
}

#[test]
fn arm_refuses_an_existing_bundle_case_insensitively() -> Result<()> {
    let bundle = "doc/plans/briefs/Foo";
    assert_clash(
        KickoffSize::Plan,
        "foo",
        &format!("{bundle}/index.md"),
        bundle,
    )
}

#[test]
fn a_bundle_named_like_a_plan_is_not_a_plan_slug_clash() -> Result<()> {
    let (root, dir) = fixture()?;
    fs::create_dir_all(root.path().join("doc/plans/feature/briefs/PLAN-new.md"))?;
    start(root.path(), &dir)?;
    assert!(arm_kickoff(&dir, root.path(), KickoffSize::Plan, "new").is_ok());
    Ok(())
}

#[test]
fn snapshot_includes_bundles_below_nested_briefs_directories() -> Result<()> {
    let root = tempdir()?;
    fs::create_dir_all(root.path().join("doc/plans/feature/archive/briefs/nested"))?;
    let SessionScope::Kickoff(scope) = SessionScope::kickoff(root.path())? else {
        unreachable!()
    };
    assert_eq!(
        scope.existing,
        vec![
            PathBuf::from("doc/plans/feature"),
            "doc/plans/feature/archive/briefs/nested".into(),
        ]
    );
    Ok(())
}

#[test]
fn arm_refuses_a_second_grant() -> Result<()> {
    let (root, dir) = fixture()?;
    start(root.path(), &dir)?;
    arm_kickoff(&dir, root.path(), KickoffSize::Feature, "first")?;
    let result = arm_kickoff(&dir, root.path(), KickoffSize::Plan, "second");
    assert_eq!(result, Err(ArmRefusal::AlreadyConfirmed));
    Ok(())
}

#[test]
fn arm_refuses_non_kickoff_scopes() -> Result<()> {
    let (root, dir) = fixture()?;
    for scope in [
        SessionScope::Open,
        SessionScope::team(vec!["doc/a.md".into()], vec![])?,
    ] {
        write(&dir, &scope)?;
        let result = arm_kickoff(&dir, root.path(), KickoffSize::Feature, "new");
        assert_eq!(result, Err(ArmRefusal::NotKickoff));
    }
    Ok(())
}

#[test]
fn snapshot_records_links_without_listing_their_targets() -> Result<()> {
    let (root, dir) = fixture()?;
    let outside = tempdir()?;
    put(outside.path(), "PLAN-hidden.md")?;
    fs::create_dir_all(root.path().join("doc/product"))?;
    fs::create_dir_all(root.path().join("doc/plans/feature/briefs"))?;
    symlink(outside.path(), root.path().join("doc/product/linked"))?;
    symlink(outside.path(), root.path().join("doc/plans/linked-home"))?;
    symlink("missing", root.path().join("doc/plans/PLAN-linked.md"))?;
    symlink(
        outside.path(),
        root.path().join("doc/plans/feature/briefs/linked"),
    )?;
    let scope = SessionScope::kickoff(root.path())?;
    let SessionScope::Kickoff(kickoff) = &scope else {
        unreachable!()
    };
    assert_eq!(
        kickoff.existing,
        vec![
            PathBuf::from("doc/plans/PLAN-linked.md"),
            "doc/plans/feature".into(),
            "doc/plans/feature/briefs/linked".into(),
            "doc/plans/linked-home".into(),
            "doc/product/linked".into()
        ]
    );
    write(&dir, &scope)?;
    let result = arm_kickoff(&dir, root.path(), KickoffSize::Feature, "linked");
    assert_eq!(result, Err(ArmRefusal::Exists("doc/product/linked".into())));
    let result = arm_kickoff(&dir, root.path(), KickoffSize::Plan, "linked");
    assert_eq!(
        result,
        Err(ArmRefusal::Exists("doc/plans/PLAN-linked.md".into()))
    );
    Ok(())
}

#[path = "scope_tests_io.rs"]
mod io;
