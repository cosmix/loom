//! Refreshing a loaded project after a non-Markdown brief file changes.

use std::path::PathBuf;

use super::{doc, section, vision, Fixture, PLAN_FIRST, VISION};
use crate::planning::trace::SettleState;
use crate::planning::watch::Batch;

#[test]
fn refreshing_a_changed_non_markdown_brief_matches_a_fresh_load_and_reads_revised() {
    let schema = "doc/plans/demo/briefs/first/build/schema.json";
    let fx = Fixture::new(&[
        (VISION, &vision(&[1])),
        (&section(1), "# S1\n\n## One\n\nText.\n"),
        (PLAN_FIRST, "# First\n\nImplements: section_01\n\nBody.\n"),
        (schema, "{\"a\": 1}\n"),
    ]);
    fx.settle(PLAN_FIRST);
    let mut project = fx.load();
    let trace = &project.feature("demo").unwrap().trace;
    assert_eq!(doc(trace, PLAN_FIRST).settle, SettleState::Settled);
    fx.put(schema, "{\"a\": 2}\n");
    let batch = Batch {
        changed: vec![PathBuf::from(schema)],
        ..Batch::default()
    };
    project.refresh(&batch).unwrap();
    assert_eq!(project, fx.load());
    let trace = &project.feature("demo").unwrap().trace;
    assert_eq!(doc(trace, PLAN_FIRST).settle, SettleState::Revised);
}
