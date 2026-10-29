//! Authoring scope enforcement through the public guard decision.

use std::fs;
use std::path::Path;

use anyhow::Result;

use super::tests::{payload, Fixture};
use super::{decide, load_policy, Decision, GuardPolicy};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::scope::{self, KickoffGrant, KickoffScope, KickoffSize, SessionScope};

fn assert_deferred(policy: &GuardPolicy, path: &Path) {
    for tool in ["Edit", "Write"] {
        assert_eq!(
            decide(policy, &payload(tool, path)),
            Decision::Defer,
            "{tool} {}",
            path.display()
        );
    }
}

fn assert_denied(policy: &GuardPolicy, path: &Path, expected: &str) {
    for tool in ["Edit", "Write"] {
        let decision = decide(policy, &payload(tool, path));
        assert!(
            matches!(&decision, Decision::Deny(reason) if reason.contains(expected)),
            "{tool} {}: expected {expected:?}, got {decision:?}",
            path.display()
        );
    }
}

#[test]
fn missing_scope_keeps_authoring_doc_writes_open() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let policy = load_policy(f.session.path())?;
    assert!(!f.session.scope().exists());

    for rel in [
        "doc/product/x/index.md",
        "doc/plans/PLAN-x.md",
        "doc/notes.md",
    ] {
        assert_deferred(&policy, &f.root.join(rel));
    }
    Ok(())
}

#[test]
fn kickoff_reloads_the_feature_grant_without_widening_it() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    scope::write(&f.session, &SessionScope::kickoff(&f.root)?)?;
    let policy = load_policy(f.session.path())?;

    for rel in [
        "doc/product/x/index.md",
        "doc/plans/PLAN-x.md",
        "doc/notes.md",
    ] {
        assert_denied(
            &policy,
            &f.root.join(rel),
            "until the size and name are confirmed",
        );
    }
    assert_deferred(&policy, &f.runtime.drafts.join("kickoff.md"));

    scope::arm_kickoff(&f.session, &f.root, KickoffSize::Feature, "x")?;
    assert_deferred(&policy, &f.root.join("doc/product/x/index.md"));
    for rel in [
        "doc/product/x/section_01.md",
        "doc/product/y/index.md",
        "doc/plans/PLAN-x.md",
        "doc/notes.md",
    ] {
        assert_denied(&policy, &f.root.join(rel), "This kickoff may write only");
    }
    Ok(())
}

#[test]
fn kickoff_plan_grants_only_the_plan_and_paths_inside_its_bundle() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    scope::write(&f.session, &SessionScope::kickoff(&f.root)?)?;
    let policy = load_policy(f.session.path())?;
    scope::arm_kickoff(&f.session, &f.root, KickoffSize::Plan, "x")?;

    for rel in ["doc/plans/PLAN-x.md", "doc/plans/briefs/x/brief.md"] {
        assert_deferred(&policy, &f.root.join(rel));
    }
    for rel in ["doc/plans/briefs/x", "doc/plans/PLAN-y.md"] {
        assert_denied(&policy, &f.root.join(rel), "This kickoff may write only");
    }
    Ok(())
}

#[test]
fn team_grants_only_listed_documents_and_bundle_contents() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let scope = SessionScope::team(
        vec!["doc/product/x/section_01.md".into()],
        vec!["doc/plans/briefs/x".into()],
    )?;
    scope::write(&f.session, &scope)?;
    fs::write(f.root.join("doc/product/existing.md"), "# Existing\n")?;
    let policy = load_policy(f.session.path())?;

    for rel in ["doc/product/x/section_01.md", "doc/plans/briefs/x/brief.md"] {
        assert_deferred(&policy, &f.root.join(rel));
    }
    for rel in [
        "doc/product/y/index.md",
        "doc/product/existing.md",
        "doc/plans/briefs/x",
    ] {
        assert_denied(&policy, &f.root.join(rel), "This team may write only");
    }
    Ok(())
}

#[test]
fn malformed_scope_denies_doc_writes() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    fs::write(f.session.scope(), b"{")?;
    let policy = load_policy(f.session.path())?;

    for rel in [
        "doc/product/x/index.md",
        "doc/plans/PLAN-x.md",
        "doc/plans/PLAN-a.md",
        "doc/plans/briefs/x/brief.md",
        "doc/notes.md",
    ] {
        assert_denied(&policy, &f.root.join(rel), "cannot read the session scope");
    }
    Ok(())
}

#[test]
fn team_grant_does_not_override_a_locked_plan() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let rel = "doc/plans/IN_PROGRESS-PLAN-b.md";
    scope::write(
        &f.session,
        &SessionScope::team(vec![rel.into()], Vec::new())?,
    )?;
    let policy = load_policy(f.session.path())?;

    assert_denied(&policy, &f.root.join(rel), "is locked (running plan)");
    Ok(())
}

#[test]
fn knowledge_writes_stay_denied_under_every_scope() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let policy = load_policy(f.session.path())?;
    let path = f.root.join("doc/loom/knowledge/x.md");
    let reason = "doc/loom/knowledge is written with `loom knowledge`";
    let scopes = [
        SessionScope::Open,
        SessionScope::kickoff(&f.root)?,
        SessionScope::Kickoff(KickoffScope {
            existing: Vec::new(),
            grant: Some(KickoffGrant {
                size: KickoffSize::Feature,
                name: "x".into(),
            }),
        }),
        SessionScope::Kickoff(KickoffScope {
            existing: Vec::new(),
            grant: Some(KickoffGrant {
                size: KickoffSize::Plan,
                name: "x".into(),
            }),
        }),
        SessionScope::team(vec!["doc/product/x/index.md".into()], Vec::new())?,
    ];

    assert_denied(&policy, &path, reason);
    for scope in scopes {
        scope::write(&f.session, &scope)?;
        assert_denied(&policy, &path, reason);
    }
    fs::write(f.session.scope(), b"{")?;
    assert_denied(&policy, &path, reason);
    Ok(())
}
