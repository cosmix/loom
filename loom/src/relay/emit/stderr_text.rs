//! The human status text [`super::RelayContext::emit`] writes to stderr
//! before the machine-readable relay line
//! (`doc/plans/PLAN-loom-state-confinement.md` section 6).

use crate::relay::RequestKind;

/// Appended to every kind's advice: the relay hook's own reply is the
/// authority, and its fault replies ("Stop and report it") say something this
/// text cannot know.
const HOOK_REPLY_PRECEDENCE: &str = "A different `LOOM relay:` reply (refused, failed, \
     unavailable) overrides this; follow that reply.";

/// Render the PENDING RELAY notice for `kind`/`id`/`what`, with the end-turn
/// reminder appended when `end_turn`. The recovery advice depends on the
/// kind, because a rerun is wrong for some kinds and the only fix for others
/// (see [`recovery`]).
pub(super) fn build(kind: RequestKind, id: &str, what: &str, end_turn: bool) -> String {
    let id8 = &id[..8];
    let mut text = format!(
        "Request {id8} ({what}) is PENDING RELAY. Nothing is recorded yet.\n\
         The loom relay hook confirms receipt right after this command with `LOOM relay: \
         received {kind} {id8}`.\n\
         Keep this command's stdout unfiltered, unredirected and in the foreground: the relay \
         reads the LOOM_RELAY_V1 line from it.\n\
         {recovery}\n",
        recovery = recovery(kind, id, id8),
    );
    if end_turn {
        text.push_str("End your turn only after the confirmation.\n");
    }
    text
}

/// What to do when the confirmation line never follows, per group of kinds.
fn recovery(kind: RequestKind, id: &str, id8: &str) -> String {
    match kind {
        RequestKind::Complete | RequestKind::Check | RequestKind::FreezeContracts => format!(
            "If no `LOOM relay: received {kind} {id8}` line follows, rerun this exact command \
             alone, in the foreground, with stdout unfiltered. A rerun within 120 s follows the \
             same request and prints its `Host job:` line instead of a new `received` line; \
             follow that line. {HOOK_REPLY_PRECEDENCE}\n\
             Check later with: loom request status {id}"
        ),
        // A rerun records the note twice (deduplication is by request id), and
        // the next foreground `loom memory` call relays a leftover ticket.
        RequestKind::Memory | RequestKind::Telemetry => format!(
            "If no `LOOM relay: received {kind} {id8}` line follows, the ticket stays queued and \
             the next foreground `loom memory` call relays it; run `loom memory note --help` \
             alone, in the foreground, to relay it now. Do not rerun this command: that records \
             it twice. {HOOK_REPLY_PRECEDENCE}\n\
             Check later with: loom request status {id}"
        ),
        // Control tickets are never swept, so only a foreground rerun files
        // the request. Status first: a missing confirmation does not prove
        // non-delivery (the hook records before it prints).
        _ => format!(
            "If no `LOOM relay: received {kind} {id8}` line follows, delivery is uncertain and \
             nothing will relay this ticket later. First run `loom request status {id}` alone. \
             If it reports pending relay or not found, the request was NOT filed: rerun this \
             exact command alone, in the foreground, with stdout unfiltered (no grep, rg, sed, \
             awk, head, redirect or `&`). If it reports any other state, the request was \
             received; do not rerun it. Report a hook fault only if the rerun also prints no \
             `LOOM relay:` line. {HOOK_REPLY_PRECEDENCE}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "4f1c9e0a7b2d4c6e8f00112233445566";

    #[test]
    fn memory_notice_names_the_short_id_the_full_id_and_what() {
        let text = build(RequestKind::Memory, ID, "memory note", false);
        assert!(text.contains("Request 4f1c9e0a (memory note) is PENDING RELAY"));
        assert!(text.contains("received memory 4f1c9e0a"));
        assert!(text.contains(&format!("loom request status {ID}")));
        assert!(text.contains("Do not rerun this command"));
        assert!(text.contains("loom memory note --help"));
        assert!(!text.contains("NOT filed"));
        assert!(!text.contains("End your turn"));
    }

    #[test]
    fn control_notice_checks_status_before_the_rerun() {
        let text = build(RequestKind::FileDispute, ID, "stage dispute", false);
        assert!(text.contains("received file-dispute 4f1c9e0a"));
        assert!(text.contains("NOT filed"));
        let status = text
            .find(&format!("loom request status {ID}"))
            .expect("names the status command");
        let rerun = text
            .find("rerun this exact command")
            .expect("names the rerun");
        assert!(status < rerun, "status must precede the rerun sentence");
        assert!(!text.contains("Do not rerun this command"));
        assert!(!text.contains("Host job"));
    }

    #[test]
    fn host_job_notice_follows_the_host_job_line() {
        let text = build(RequestKind::Complete, ID, "stage complete", false);
        assert!(text.contains("received complete 4f1c9e0a"));
        assert!(text.contains("`Host job:` line"));
        assert!(!text.contains("NOT filed"));
        assert!(!text.contains("Do not rerun this command"));
    }

    #[test]
    fn no_notice_blames_the_hook_installation_or_forbids_a_retry() {
        for kind in RequestKind::all() {
            let text = build(kind, ID, "something", true);
            assert!(!text.contains("do not retry"), "{kind}");
            assert!(!text.contains("is not installed"), "{kind}");
            assert!(text.contains("A different `LOOM relay:` reply"), "{kind}");
            assert!(
                text.contains(&format!("received {kind} 4f1c9e0a")),
                "{kind}"
            );
        }
    }

    #[test]
    fn appends_the_end_turn_reminder_when_requested() {
        let text = build(RequestKind::Block, ID, "stage block", true);
        assert!(text.ends_with("End your turn only after the confirmation.\n"));
    }
}
