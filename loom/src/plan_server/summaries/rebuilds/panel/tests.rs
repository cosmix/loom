//! Overview estimates sum only the members of their own feature and document family.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use super::*;
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::summaries::spawner::RealSpawner;
use crate::planning::summary::evaluate::{evaluate, fixtures};
use crate::planning::watch::OwnWrites;

fn context(root: &Path) -> Arc<PlanContext> {
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let bind = BindInfo::new(local, "test");
    Arc::new(
        PlanContext::new(
            root.to_path_buf(),
            ReadOnly::from_problems(Vec::new()),
            bind,
            OwnWrites::default(),
        )
        .unwrap(),
    )
}

#[test]
fn overview_sizes_use_each_features_own_members() {
    let fixture = fixtures::two_features();
    let extra = fixtures::section(2, "Unlisted.", "Extra.");
    fixtures::put(&fixture.root, fixtures::S2, &extra);
    let ctx = context(&fixture.root);
    let evaluation = evaluate(&ctx.project(), &[]);
    let rebuilds = Rebuilds::with_spawner(&ctx, Arc::new(RealSpawner::new(&ctx)));

    let sizes = rebuilds.source_sizes(&evaluation).unwrap();
    let overviews: BTreeMap<_, _> = sizes
        .into_iter()
        .filter(|(id, _)| is_overview(id))
        .collect();
    let expected = [
        (
            "overview/demo/product",
            fixtures::section(1, "Alpha.", "Beta.").len(),
        ),
        ("overview/demo/engineering", "# Plan: X\n\nDoes x.\n".len()),
        (
            "overview/other/product",
            fixtures::section(1, "Other alpha.", "Other beta.").len(),
        ),
        (
            "overview/other/engineering",
            "# Plan: Y\n\nDoes y with more detail.\n".len(),
        ),
    ];
    let expected: BTreeMap<_, _> = expected
        .into_iter()
        .map(|(id, bytes)| (id.to_owned(), u64::try_from(bytes).unwrap()))
        .collect();

    assert_eq!(overviews, expected);
}
