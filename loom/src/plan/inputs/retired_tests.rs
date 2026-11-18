//! Contract: a `RETIRED-` plan's brief bundle is one of its inputs.

use super::collect_inputs;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn retired_plan_bundle_is_an_input() {
    let temp = tempfile::Builder::new()
        .prefix("loom-planning-")
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let root = temp.path();
    let plan = "doc/plans/archive/RETIRED-PLAN-foo.md";
    let brief = format!("doc/plans/{}/foo/common.md", "briefs");
    for (path, text) in [(plan, "# Retired plan\n"), (brief.as_str(), "# Common\n")] {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let inputs = collect_inputs(root, Path::new(plan)).unwrap();
    assert!(
        inputs.files.contains_key(&PathBuf::from(&brief)),
        "the RETIRED plan's bundle must be an input: {:?}",
        inputs.files.keys().collect::<Vec<_>>()
    );
}
