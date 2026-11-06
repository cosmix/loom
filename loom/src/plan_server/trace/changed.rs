//! Which features' trace views changed in a watcher batch.

use std::collections::{BTreeMap, BTreeSet};

use crate::plan_server::wire::trace::TraceView;
use crate::planning::project::Project;

/// Every feature's trace view, by feature name.
pub fn views(project: &Project) -> BTreeMap<String, TraceView> {
    project
        .features
        .iter()
        .map(|feature| (feature.name.clone(), TraceView::from(&feature.trace)))
        .collect()
}

/// The features whose view differs between `before` and `after`, including a feature that
/// appeared or went away (the browser refetches and gets its answer, 404 for a gone feature).
pub fn changed(
    before: &BTreeMap<String, TraceView>,
    after: &BTreeMap<String, TraceView>,
) -> Vec<String> {
    before
        .keys()
        .chain(after.keys())
        .filter(|name| before.get(*name) != after.get(*name))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
