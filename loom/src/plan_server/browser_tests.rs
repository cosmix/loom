//! Tests for the browser opener.

use super::browser::{decide, EnvView, OpenDecision, SkipReason};

#[test]
fn decision_respects_skip_order_and_display_options() {
    let mut env = EnvView {
        ssh_connection: false,
        ssh_tty: false,
        display: false,
        wayland: false,
        linux: true,
    };
    assert_eq!(
        decide(&env, false),
        OpenDecision::Skip(SkipReason::NoDisplay)
    );
    env.ssh_connection = true;
    assert_eq!(decide(&env, false), OpenDecision::Skip(SkipReason::Ssh));
    assert_eq!(decide(&env, true), OpenDecision::Skip(SkipReason::NoOpen));
    env.ssh_connection = false;
    env.ssh_tty = true;
    assert_eq!(decide(&env, false), OpenDecision::Skip(SkipReason::Ssh));
    env.ssh_tty = false;
    env.display = true;
    assert_eq!(decide(&env, false), OpenDecision::Open);
    env.display = false;
    env.wayland = true;
    assert_eq!(decide(&env, false), OpenDecision::Open);
    env.wayland = false;
    env.linux = false;
    assert_eq!(decide(&env, false), OpenDecision::Open);
}
