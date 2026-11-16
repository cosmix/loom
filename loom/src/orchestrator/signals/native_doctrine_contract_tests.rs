//! Contract tests for the native codex lane's Codex Implementers signal text.
//!
//! `format_codex_implementers_section` selects its text from a resolved
//! [`CodexLane`]: the native lane launches workers through `loom subagents`,
//! the legacy lane keeps the forwarder spawn rules, and an unqualified native
//! backend is UNAVAILABLE with its reason, never a silent legacy fallback.

use super::format::format_codex_implementers_section;
use crate::codex::CodexLane;
use crate::models::stage::{Implementer, Implementers};

fn mixed_implementers() -> Implementers {
    Implementers::new(vec![Implementer::Codex, Implementer::Claude])
}

#[test]
fn native_lane_signal_names_start_and_no_forwarder() {
    let implementers = mixed_implementers();

    let native = format_codex_implementers_section(&implementers, &CodexLane::Native);
    assert!(
        native.contains("loom subagents start --provider codex"),
        "native text must name the start command:\n{native}"
    );
    assert!(
        native.contains("loom subagents watch --worker codex:<unit-id>"),
        "native text must name the codex worker watch:\n{native}"
    );
    assert!(
        !native.contains("subagent_type"),
        "native text must not tell the agent to spawn by subagent_type:\n{native}"
    );

    let legacy = format_codex_implementers_section(&implementers, &CodexLane::Legacy);
    assert!(
        legacy.contains("loom-codex-forwarder"),
        "legacy text must keep the forwarder spawn rules:\n{legacy}"
    );
    assert_ne!(native, legacy, "the lane must select different text");
}

#[test]
fn unqualified_native_backend_is_unavailable_not_legacy() {
    let reason = "bubblewrap cannot create a pid namespace";
    let lane = CodexLane::Unavailable(reason.to_string());

    let text = format_codex_implementers_section(&mixed_implementers(), &lane);
    assert!(
        text.contains(reason),
        "unavailable text must carry the reason:\n{text}"
    );
    assert!(
        text.contains("UNAVAILABLE"),
        "unavailable text must say UNAVAILABLE:\n{text}"
    );
    assert!(
        !text.contains("loom subagents start"),
        "unavailable text must not launch native workers:\n{text}"
    );
    assert!(
        !text.contains("subagent_type"),
        "unavailable text must not fall back to forwarder spawns:\n{text}"
    );
}
