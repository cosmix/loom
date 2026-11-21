use super::*;

fn config_with(plan: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), format!("[plan]\n{plan}")).unwrap();
    dir
}

#[test]
fn the_slug_drops_the_lifecycle_prefix_and_the_extension() {
    assert_eq!(
        source_slug("doc/plans/IN_PROGRESS-PLAN-Contract_demo v2.md"),
        "plan-contract-demo-v2"
    );
    assert_eq!(source_slug("doc/plans/DONE-PLAN-x.md"), "plan-x");
    assert_eq!(source_slug("PLAN-x"), "plan-x");
}

#[test]
fn the_slug_is_bounded_and_never_empty() {
    assert_eq!(source_slug("../.."), "plan");
    assert_eq!(source_slug("plans/---.md"), "plan");
    assert_eq!(
        source_slug(&format!("{}.md", "a".repeat(200))).len(),
        MAX_SLUG_LEN
    );
    let ends_dashed = format!("{}-b.md", "a".repeat(MAX_SLUG_LEN - 1));
    assert!(!source_slug(&ends_dashed).ends_with('-'));
}

#[test]
fn a_stamped_table_carries_a_complete_identity() {
    let mut table = Table::new();
    stamp_new_run(&mut table, "doc/plans/IN_PROGRESS-PLAN-demo.md");
    let identity = parse_identity(|key| table.get(key).and_then(Item::as_str)).unwrap();
    let id = identity.plan_id();
    assert!(id.contains("-plan-demo-"), "{id}");
    assert_eq!(id.len(), 16 + "plan-demo".len() + 2 + 36);
}

#[test]
fn two_stamps_of_one_source_never_share_an_id() {
    let (mut a, mut b) = (Table::new(), Table::new());
    stamp_new_run(&mut a, "PLAN-x.md");
    stamp_new_run(&mut b, "PLAN-x.md");
    let id = |table: &Table| {
        parse_identity(|key| table.get(key).and_then(Item::as_str))
            .unwrap()
            .plan_id()
    };
    assert_ne!(id(&a), id(&b));
}

#[test]
fn a_run_without_an_identity_is_assigned_one_once() {
    let dir = config_with("source_path = \"doc/plans/IN_PROGRESS-PLAN-old.md\"\n");
    assert_eq!(read(dir.path()).unwrap(), None);
    let first = ensure(dir.path()).unwrap();
    assert_eq!(ensure(dir.path()).unwrap(), first, "a retry reuses it");
    assert_eq!(read(dir.path()).unwrap(), Some(first.clone()));
    assert!(first.plan_id().contains("-plan-old-"));
}

#[test]
fn only_the_missing_or_malformed_keys_are_reassigned() {
    let run = Uuid::new_v4();
    let dir = config_with(&format!(
        "source_path = \"PLAN-x.md\"\nstats_run_id = \"{run}\"\n\
         stats_initialized_at = \"2026-11-21T12:00:00Z\"\nstats_source_slug = \"Not Valid\"\n"
    ));
    let identity = ensure(dir.path()).unwrap();
    assert_eq!(identity.run_id, run);
    assert_eq!(identity.slug, "plan-x");
    assert!(identity.plan_id().starts_with("20261121T120000Z-plan-x-"));
}

#[test]
fn a_state_without_a_plan_table_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), "[other]\nkey = 1\n").unwrap();
    assert!(ensure(dir.path()).is_err());
    assert_eq!(read(dir.path()).unwrap(), None);
}
