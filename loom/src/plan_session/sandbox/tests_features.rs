//! Feature-home confinement and deny-list regressions.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

use anyhow::Result;
use serde_json::{json, Value};

use super::tests_kinds::Fixture;
use crate::plan_session::kind::SessionKind;

fn seed_home(root: &Path, home: &str) -> Result<()> {
    let home = root.join(home);
    for dir in ["archive", "briefs/z", "briefs/w"] {
        fs::create_dir_all(home.join(dir))?;
    }
    for file in [
        "IN_PROGRESS-PLAN-z.md",
        "PLAN-w.md",
        "REVIEW-PLAN-z.md",
        "archive/DONE-PLAN-q.md",
        "briefs/z/input.md",
        "briefs/w/input.md",
    ] {
        fs::write(home.join(file), "# Fixture\n")?;
    }
    Ok(())
}

fn edit(root: &Path, rel: &str) -> Value {
    json!(format!("Edit(/{}/{rel})", root.display()))
}

#[test]
fn authoring_denies_feature_locks_and_missing_archived_bundles() -> Result<()> {
    let f = Fixture::new();
    seed_home(&f.root, "doc/plans/alpha")?;
    // Even an empty feature gets rules for future archives and run reviews.
    fs::create_dir(f.root.join("doc/plans/beta"))?;
    let settings = f.build(&SessionKind::Authoring, &[])?;
    let os = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();
    for rel in [
        "doc/plans/alpha/IN_PROGRESS-PLAN-z.md",
        "doc/plans/alpha/REVIEW-PLAN-z.md",
        "doc/plans/alpha/archive",
        "doc/plans/alpha/briefs/z",
    ] {
        assert!(os.contains(&json!(f.root.join(rel))), "{rel}");
    }
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    for rel in [
        "doc/plans/alpha/IN_PROGRESS-PLAN-z.md",
        "doc/plans/alpha/archive/**",
        "doc/plans/alpha/REVIEW-*.md",
        "doc/plans/alpha/briefs/z/**",
        "doc/plans/alpha/briefs/q/**",
        "doc/plans/beta/archive/**",
        "doc/plans/beta/REVIEW-*.md",
    ] {
        assert!(deny.contains(&edit(&f.root, rel)), "{rel}");
    }
    for rel in ["doc/plans/alpha/briefs/w/**", "doc/plans/briefs/z/**"] {
        assert!(!deny.contains(&edit(&f.root, rel)), "{rel}");
    }
    assert!(!os.contains(&json!(f.root.join("doc/plans/alpha/briefs/w"))));
    Ok(())
}

#[test]
fn feature_pressure_allows_its_bundle_and_denies_siblings() -> Result<()> {
    let f = Fixture::new();
    seed_home(&f.root, "doc/plans/alpha")?;
    let kind = SessionKind::Pressure {
        plan: "doc/plans/alpha/PLAN-w.md".into(),
    };
    let settings = f.build(&kind, &[])?;
    let allow = settings["permissions"]["allow"].as_array().unwrap();
    for rel in ["doc/plans/alpha/PLAN-w.md", "doc/plans/alpha/briefs/w/**"] {
        assert!(allow.contains(&edit(&f.root, rel)), "{rel}");
    }
    assert!(!allow.contains(&edit(&f.root, "doc/plans/briefs/w/**")));
    let os = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();
    for rel in [
        "doc/plans/PLAN-a.md",
        "doc/plans/briefs",
        "doc/plans/alpha/archive",
        "doc/plans/alpha/IN_PROGRESS-PLAN-z.md",
        "doc/plans/alpha/briefs/z",
    ] {
        assert!(os.contains(&json!(f.root.join(rel))), "{rel}");
    }
    for entry in os {
        let denied = Path::new(entry.as_str().unwrap());
        for rel in [
            "doc/plans/alpha/PLAN-w.md",
            "doc/plans/alpha/briefs/w/new.md",
        ] {
            assert!(!f.root.join(rel).starts_with(denied), "{entry}");
        }
    }
    Ok(())
}

#[test]
fn pressure_settings_refuse_unsafe_slugs_and_invalid_features() {
    let f = Fixture::new();
    for rel in [
        "doc/plans/PLAN-..md",
        "doc/plans/PLAN-...md",
        "doc/plans/alpha/PLAN-..md",
        "doc/plans/alpha/PLAN-...md",
        "doc/plans/Bad/PLAN-z.md",
        "doc/plans/bad_name/PLAN-z.md",
        "doc/plans/-bad/PLAN-z.md",
        "doc/plans/briefs/PLAN-z.md",
        "doc/plans/archive/PLAN-z.md",
        "doc/plans/alpha/archive/PLAN-z.md",
        "doc/plans/alpha/nested/PLAN-z.md",
    ] {
        let kind = SessionKind::Pressure { plan: rel.into() };
        assert!(f.build(&kind, &[]).is_err(), "{rel}");
    }
}

#[test]
fn feature_denies_leave_standalone_lists_unchanged() -> Result<()> {
    let f = Fixture::new();
    let before = f.build(&SessionKind::Authoring, &[])?;
    seed_home(&f.root, "doc/plans/alpha")?;
    let after = f.build(&SessionKind::Authoring, &[])?;
    for (old, new) in [
        (
            &before["permissions"]["deny"],
            &after["permissions"]["deny"],
        ),
        (
            &before["sandbox"]["filesystem"]["denyWrite"],
            &after["sandbox"]["filesystem"]["denyWrite"],
        ),
    ] {
        let retained: Vec<_> = new
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| !entry.as_str().unwrap().contains("/doc/plans/alpha/"))
            .cloned()
            .collect();
        assert_eq!(old, &json!(retained));
    }
    Ok(())
}

#[test]
fn feature_home_and_bundle_symlinks_are_not_followed() -> Result<()> {
    let f = Fixture::new();
    seed_home(&f.root, "doc/plans/alpha")?;
    let external = f.home.join("external");
    fs::create_dir_all(external.join("briefs"))?;
    symlink(&external, f.root.join("doc/plans/linked"))?;
    fs::remove_dir_all(f.root.join("doc/plans/alpha/briefs/z"))?;
    symlink(&external, f.root.join("doc/plans/alpha/briefs/z"))?;
    let settings = f.build(&SessionKind::Authoring, &[])?;
    let deny = settings["permissions"]["deny"].as_array().unwrap();
    assert!(!deny.contains(&edit(&f.root, "doc/plans/linked/archive/**")));
    // The lexical bundle stays denied, without listing anything through its link.
    assert!(deny.contains(&edit(&f.root, "doc/plans/alpha/briefs/z/**")));
    let os = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();
    assert!(!os.iter().any(|entry| entry
        .as_str()
        .unwrap()
        .starts_with(external.to_str().unwrap())));
    Ok(())
}

#[test]
fn feature_pressure_denies_a_symlinked_home_by_its_own_path() -> Result<()> {
    let f = Fixture::new();
    let external = f.home.join("external");
    fs::create_dir_all(external.join("briefs/z"))?;
    fs::write(external.join("PLAN-z.md"), "# Fixture\n")?;
    let link = f.root.join("doc/plans/alpha");
    symlink(&external, &link)?;
    let kind = SessionKind::Pressure {
        plan: "doc/plans/alpha/PLAN-z.md".into(),
    };

    let settings = f.build(&kind, &[])?;

    let os = settings["sandbox"]["filesystem"]["denyWrite"]
        .as_array()
        .unwrap();
    assert!(os.contains(&json!(link)));
    assert!(!os.contains(&json!(external)));
    assert!(!os.contains(&json!(link.join("briefs"))));
    Ok(())
}
