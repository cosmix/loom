use super::*;

#[test]
fn forward_state_labels_are_stable() {
    assert_eq!(SubagentState::Failed.label(), "failed");
    assert_eq!(SubagentState::Cancelled.label(), "cancelled");
    assert_eq!(SubagentState::ForwardWait.label(), "forward-wait");
    assert_eq!(SubagentState::ForwardFailed.label(), "forward-failed");
    assert_eq!(SubagentState::ForwardUnknown.label(), "forward-unknown");
}
