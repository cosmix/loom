//! The failure line prints the scores unrounded, so a miss of a few thousandths
//! does not read as `0.40 < 0.40`.

use super::report::{exit_reason, Aggregates};

#[test]
fn a_precision_just_under_the_floor_is_not_rounded_up() {
    let aggregates = Aggregates {
        hit_rate_at_5: 1.0,
        precision_at_5: 0.3958333,
        ..Aggregates::default()
    };

    let reason = exit_reason(&aggregates, 0.5, 0.4).expect("precision is under the floor");

    assert!(reason.contains("0.3958333"), "{reason}");
    assert!(reason.contains("precision_floor 0.4"), "{reason}");
}
