//! Feature-home pressure write sets and live locks.

use std::fs;

use anyhow::Result;

use super::tests::{payload, Fixture};
use super::{decide, load_policy, Decision};
use crate::plan_session::kind::SessionKind;

#[test]
fn feature_pressure_defers_only_its_plan_and_bundle() -> Result<()> {
    let f = Fixture::new(SessionKind::Pressure {
        plan: "doc/plans/alpha/PLAN-z.md".into(),
    })?;
    fs::create_dir_all(f.root.join("doc/plans/alpha/briefs/z"))?;
    fs::write(f.root.join("doc/plans/alpha/PLAN-z.md"), "# Fixture\n")?;

    for rel in [
        "doc/plans/alpha/PLAN-z.md",
        "doc/plans/alpha/briefs/z/new.md",
        "doc/plans/alpha/briefs/z/nested/new.md",
    ] {
        f.write(&f.root.join(rel), false);
    }
    for rel in [
        "doc/plans/briefs/z/new.md",
        "doc/plans/beta/briefs/z/new.md",
        "doc/plans/alpha/briefs/other/new.md",
        "doc/plans/alpha/PLAN-other.md",
        "doc/plans/alpha/archive/new.md",
        "doc/plans/alpha/REVIEW-PLAN-z.md",
    ] {
        f.write(&f.root.join(rel), true);
    }
    // The live locked set still takes precedence over the pressure grant.
    fs::rename(
        f.root.join("doc/plans/alpha/PLAN-z.md"),
        f.root.join("doc/plans/alpha/IN_PROGRESS-PLAN-z.md"),
    )?;
    f.write(&f.root.join("doc/plans/alpha/briefs/z/new.md"), true);
    f.write(&f.root.join("doc/plans/alpha/IN_PROGRESS-PLAN-z.md"), true);
    Ok(())
}

#[test]
fn authoring_denies_new_files_in_a_locked_feature_bundle() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    fs::create_dir_all(f.root.join("doc/plans/alpha/briefs/z"))?;
    fs::write(
        f.root.join("doc/plans/alpha/IN_PROGRESS-PLAN-z.md"),
        "# Fixture\n",
    )?;

    for rel in [
        "doc/plans/alpha/briefs/z/new.md",
        "doc/plans/alpha/archive/new.md",
        "doc/plans/alpha/REVIEW-PLAN-z.md",
    ] {
        f.write(&f.root.join(rel), true);
    }
    f.write(&f.root.join("doc/plans/briefs/z/new.md"), false);
    Ok(())
}

#[test]
fn malformed_pressure_targets_never_grant_the_plan_or_bundle() -> Result<()> {
    let f = Fixture::new(SessionKind::Authoring)?;
    let mut policy = load_policy(f.session.path())?;
    for (plan, attempted) in [
        ("doc/plans/alpha/PLAN-..md", "doc/plans/alpha/briefs/new.md"),
        ("doc/plans/alpha/PLAN-...md", "doc/plans/alpha/new.md"),
        ("doc/plans/PLAN-..md", "doc/plans/briefs/new.md"),
        ("doc/plans/PLAN-...md", "doc/plans/new.md"),
        ("doc/plans/Bad/PLAN-z.md", "doc/plans/Bad/briefs/z/new.md"),
        (
            "doc/plans/bad_name/PLAN-z.md",
            "doc/plans/bad_name/briefs/z/new.md",
        ),
        ("doc/plans/-bad/PLAN-z.md", "doc/plans/-bad/briefs/z/new.md"),
    ] {
        policy.kind = SessionKind::Pressure { plan: plan.into() };
        for rel in [plan, attempted] {
            let input = payload("Write", &f.root.join(rel));
            assert!(
                matches!(decide(&policy, &input), Decision::Deny(_)),
                "{rel}"
            );
        }
    }
    Ok(())
}
