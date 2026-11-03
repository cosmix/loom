//! Contract for the overview model default: sonnet when unset, the explicit value otherwise.

use super::{parse_document, UserConfig};

#[test]
fn overview_model_defaults_to_sonnet() {
    let unset = parse_document("[plan]\noverview_effort = \"high\"\n").unwrap();
    assert_eq!(unset.plan_overview_model(), "sonnet");
    assert_eq!(UserConfig::default().plan_overview_model(), "sonnet");

    let set = parse_document("[plan]\noverview_model = \"haiku\"\n").unwrap();
    assert_eq!(set.plan_overview_model(), "haiku");
}
