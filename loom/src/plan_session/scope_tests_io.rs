use super::*;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn scope_file_round_trips_large_kickoff_and_replaces_with_private_permissions() -> Result<()> {
    let (_root, dir) = fixture()?;
    for scope in [
        SessionScope::Open,
        SessionScope::Kickoff(KickoffScope {
            existing: (0..3000)
                .map(|i| format!("doc/plans/PLAN-entry-{i}.md").into())
                .collect(),
            grant: None,
        }),
        SessionScope::team(vec!["doc/a.md".into()], vec!["doc/plans/briefs/a".into()])?,
    ] {
        write(&dir, &scope)?;
        assert_eq!(load(&dir)?, scope);
        assert_eq!(
            fs::metadata(dir.scope())?.permissions().mode() & 0o777,
            0o600
        );
        fs::set_permissions(dir.scope(), fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

#[test]
fn load_refuses_malformed_unknown_fields_and_oversize_files() -> Result<()> {
    let (_root, dir) = fixture()?;
    for text in [
        "{",
        r#"{"scope":"unknown"}"#,
        r#"{"scope":"open","extra":true}"#,
        r#"{"scope":"kickoff","existing":[],"grant":null,"extra":true}"#,
        r#"{"scope":"team","documents":[],"bundles":[]}"#,
    ] {
        fs::write(dir.scope(), text)?;
        assert!(load(&dir).is_err(), "accepted {text}");
    }
    let oversized = format!("{}{}", r#"{"scope":"open"}"#, " ".repeat(MAX_SCOPE_BYTES));
    fs::write(dir.scope(), oversized)?;
    assert!(load(&dir).is_err());
    Ok(())
}

#[test]
fn missing_scope_is_open() -> Result<()> {
    let (_root, dir) = fixture()?;
    assert_eq!(load(&dir)?, SessionScope::Open);
    Ok(())
}

#[test]
fn load_accepts_a_scope_at_exactly_the_size_limit() -> Result<()> {
    let (_root, dir) = fixture()?;
    let mut text = String::from(r#"{"scope":"open"}"#);
    text.push_str(&" ".repeat(MAX_SCOPE_BYTES - text.len()));
    fs::write(dir.scope(), text)?;
    assert_eq!(load(&dir)?, SessionScope::Open);
    Ok(())
}

#[test]
fn scope_file_links_are_refused_without_modifying_the_target() -> Result<()> {
    let (root, dir) = fixture()?;
    let outside = root.path().join("outside.json");
    fs::write(&outside, r#"{"scope":"open"}"#)?;
    symlink(&outside, dir.scope())?;
    assert!(load(&dir).is_err());
    assert!(write(&dir, &SessionScope::Open).is_err());
    assert_eq!(fs::read_to_string(outside)?, r#"{"scope":"open"}"#);
    Ok(())
}

#[test]
fn arm_reports_an_unreadable_scope() -> Result<()> {
    let (root, dir) = fixture()?;
    fs::write(dir.scope(), "malformed")?;
    let result = arm_kickoff(&dir, root.path(), KickoffSize::Plan, "new");
    assert!(matches!(result, Err(ArmRefusal::Unreadable(_))));
    Ok(())
}

#[test]
fn team_refuses_empty_documents_and_invalid_paths_in_both_lists() {
    assert!(SessionScope::team(vec![], vec![]).is_err());
    for path in [
        "/doc/a.md",
        "doc/../a.md",
        "doc/./a.md",
        "./doc/a.md",
        "a.md",
        "docs/a.md",
        "doc",
        "doc/loom",
        "doc/loom/a.md",
    ] {
        let document = SessionScope::team(vec![path.into()], vec![]);
        assert!(document.is_err(), "accepted {path}");
        let bundle = SessionScope::team(vec!["doc/a.md".into()], vec![path.into()]);
        assert!(bundle.is_err(), "accepted bundle {path}");
    }
}

#[test]
fn team_refuses_duplicates_within_and_across_lists() {
    assert!(SessionScope::team(vec!["doc/a.md".into(), "doc/a.md".into()], vec![]).is_err());
    assert!(SessionScope::team(
        vec!["doc/a.md".into()],
        vec!["doc/bundle".into(), "doc/bundle".into()]
    )
    .is_err());
    assert!(SessionScope::team(vec!["doc/a.md".into()], vec!["doc/a.md".into()]).is_err());
}

#[test]
fn team_accepts_at_most_64_documents() {
    let documents: Vec<PathBuf> = (0..64).map(|i| format!("doc/{i}.md").into()).collect();
    assert!(SessionScope::team(documents.clone(), vec![]).is_ok());
    let mut too_many = documents;
    too_many.push("doc/64.md".into());
    assert!(SessionScope::team(too_many, vec![]).is_err());
}

#[test]
fn team_allows_only_listed_documents_and_strict_bundle_descendants() -> Result<()> {
    let scope = SessionScope::team(
        vec!["doc/plans/feature/PLAN-one.md".into()],
        vec!["doc/plans/feature/briefs/one".into()],
    )?;
    assert!(scope.peer_tools());
    for path in [
        "doc/plans/feature/PLAN-one.md",
        "doc/plans/feature/briefs/one/brief.md",
        "doc/plans/feature/briefs/one/nested/unit.md",
    ] {
        assert_eq!(scope.allows_write(Path::new(path)), Ok(()));
    }
    for path in [
        "doc/plans/feature/PLAN-two.md",
        "doc/plans/feature/briefs/one",
        "doc/plans/feature/briefs/one-more/brief.md",
    ] {
        assert_denied(&scope, path, "only the documents this run was started for");
    }
    Ok(())
}
