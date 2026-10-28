use serde_json::json;

use super::tests::{at, event, feed, request, start, stop, subagent, working};
use super::*;

#[test]
fn subagent_stop_with_none_running_stays_at_zero() {
    let mut machine = working();
    feed(&mut machine, subagent("SubagentStop", 1_000));
    assert_eq!(machine.state(), SessionState::Working);
    assert_eq!(machine.subagents_running(), 0);
    feed(&mut machine, subagent("SubagentStart", 1_100));
    feed(&mut machine, subagent("SubagentStop", 1_200));
    assert_eq!(machine.state(), SessionState::Working);
    feed(&mut machine, subagent("SubagentStop", 1_300));
    assert_eq!(machine.state(), SessionState::Working);
    assert_eq!(machine.subagents_running(), 0);
}

#[test]
fn the_subagent_count_resets_on_interrupt_agents_killed_and_session_start() {
    let resets: [fn(&mut StateMachine); 3] = [
        |machine| {
            machine.apply(Input::Interrupted { at: at(2_000) });
        },
        |machine| {
            machine.apply(Input::AgentsKilled { at: at(2_000) });
        },
        |machine| {
            start(machine, "startup", 2_000);
        },
    ];
    for reset in resets {
        let mut machine = working();
        feed(&mut machine, subagent("SubagentStart", 1_000));
        feed(&mut machine, subagent("SubagentStart", 1_100));
        assert_eq!(machine.subagents_running(), 2);
        reset(&mut machine);
        assert_eq!(machine.subagents_running(), 0);
    }
}

#[test]
fn ending_is_sticky_while_activity_tracks_underneath() {
    let mut machine = working();
    assert_eq!(
        machine.apply(Input::EndRequested),
        Some(SessionState::Ending)
    );
    assert_eq!(machine.activity(), Activity::Working);
    assert_eq!(feed(&mut machine, stop(1_000)), None);
    assert_eq!(
        (machine.state(), machine.activity()),
        (SessionState::Ending, Activity::Idle)
    );
    feed(&mut machine, request("r1", 1_100));
    assert_eq!(machine.activity(), Activity::Permission);
    machine.apply(Input::PermissionAnswered {
        request_id: "r1".into(),
    });
    assert_eq!(
        (machine.state(), machine.activity()),
        (SessionState::Ending, Activity::Working)
    );
}

#[test]
fn ending_before_the_start_is_not_idle() {
    let mut machine = StateMachine::new();
    machine.apply(Input::EndRequested);
    assert_eq!(machine.activity(), Activity::Working);
    start(&mut machine, "startup", 0);
    assert_eq!(
        (machine.state(), machine.activity()),
        (SessionState::Ending, Activity::Idle)
    );
}

#[test]
fn session_end_moves_to_ended() {
    let mut machine = working();
    machine.apply(Input::EndRequested);
    let changed = feed(
        &mut machine,
        event("SessionEnd", 1_000, json!({"reason": "exit"})),
    );
    assert_eq!(changed, Some(SessionState::Ended));
}

#[test]
fn process_gone_while_ending_is_ended() {
    let mut machine = working();
    machine.apply(Input::EndRequested);
    assert_eq!(machine.apply(Input::ProcessGone), Some(SessionState::Ended));
}

#[test]
fn process_gone_outside_ending_is_failed() {
    let mut machine = working();
    assert_eq!(
        machine.apply(Input::ProcessGone),
        Some(SessionState::Failed)
    );
}

#[test]
fn spawn_failed_is_failed() {
    let mut machine = StateMachine::new();
    assert_eq!(
        machine.apply(Input::SpawnFailed),
        Some(SessionState::Failed)
    );
}

#[test]
fn nothing_moves_a_finished_machine() {
    let mut machine = working();
    machine.apply(Input::ProcessGone);
    assert_eq!(start(&mut machine, "startup", 2_000), None);
    assert_eq!(machine.apply(Input::EndRequested), None);
    assert_eq!(machine.state(), SessionState::Failed);
}

#[test]
fn a_self_started_turn_counts_only_after_the_last_idle_event() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    assert_eq!(machine.state(), SessionState::Idle);
    assert_eq!(machine.apply(Input::SelfStartedTurn { at: at(500) }), None);
    assert_eq!(machine.state(), SessionState::Idle);
    assert_eq!(
        machine.apply(Input::SelfStartedTurn { at: at(2_000) }),
        Some(SessionState::Working)
    );
}
