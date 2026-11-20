//! Contract: the kickoff opening names the size the session was started for.

use super::{opening_message, AuthoringStart};
use crate::planning::guidance::team::tests::fixture;

/// Contract `kickoff-opening-names-the-size`: the feature kickoff opens with
/// "loom plan: Kickoff, feature." and the small one with "loom plan: Kickoff, standalone plan.";
/// an opening that says only "Kickoff" fails.
#[test]
fn kickoff_opening_names_the_size() {
    let (_temp, project) = fixture();
    let opening =
        |small| opening_message(&AuthoringStart::Kickoff { small }, &project, Some("f")).unwrap();
    let (feature, standalone) = (opening(false), opening(true));

    assert!(
        feature.starts_with("loom plan: Kickoff, feature.\n"),
        "feature opening: {feature}"
    );
    assert!(
        standalone.starts_with("loom plan: Kickoff, standalone plan.\n"),
        "standalone opening: {standalone}"
    );
    assert_ne!(feature, standalone);
    assert!(!feature.starts_with("loom plan: Kickoff.\n"), "{feature}");
}
