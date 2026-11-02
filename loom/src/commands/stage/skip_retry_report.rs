//! Operator-facing retry output.

pub(super) fn recovery_started(
    stage_id: &str,
    previous_session_id: &str,
    new_session_id: &str,
    reason: &impl std::fmt::Display,
    recovery_attempt: u32,
    context: Option<&str>,
    last_event: Option<&str>,
) {
    println!("Recovery initiated for stage '{stage_id}'");
    println!("  Previous session: {previous_session_id}");
    println!("  New session: {new_session_id}");
    println!("  Recovery reason: {reason}");
    println!("  Attempt: #{recovery_attempt}");
    if let Some(message) = context {
        println!("  Context: {message}");
    }
    if let Some(event) = last_event {
        println!("  Last hook event: {event}");
    }
    println!();
    println!("The stage will be picked up by the next orchestrator poll.");
    println!("Run 'loom run' to start the orchestrator if not running.");
}
