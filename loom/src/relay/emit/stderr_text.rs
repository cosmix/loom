//! The human status text [`super::RelayContext::emit`] writes to stderr
//! before the machine-readable relay line
//! (`doc/plans/PLAN-loom-state-confinement.md` section 6).

/// Render the five-line PENDING RELAY notice for `id`/`what`, with the
/// end-turn reminder appended when `end_turn`.
pub(super) fn build(id: &str, what: &str, end_turn: bool) -> String {
    let id8 = &id[..8];
    let mut text = format!(
        "Request {id8} ({what}) is PENDING RELAY. Nothing is recorded yet.\n\
         The loom relay hook confirms receipt right after this command; the daemon applies it \
         within a few seconds.\n\
         Keep this command's stdout unfiltered, unredirected and in the foreground: the relay \
         reads the LOOM_RELAY_V1 line from it.\n\
         If no \"LOOM relay: received {id8}\" message follows, the relay hook is not installed. \
         Stop and report it; do not retry.\n\
         Check later with: loom request status {id}\n"
    );
    if end_turn {
        text.push_str("End your turn after the confirmation.\n");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "4f1c9e0a7b2d4c6e8f00112233445566";

    #[test]
    fn includes_the_short_id_the_full_id_and_what() {
        let text = build(ID, "memory note", false);
        assert!(text.contains("Request 4f1c9e0a (memory note) is PENDING RELAY"));
        assert!(text.contains("received 4f1c9e0a"));
        assert!(text.contains(&format!("loom request status {ID}")));
        assert!(!text.contains("End your turn"));
    }

    #[test]
    fn appends_the_end_turn_reminder_when_requested() {
        let text = build(ID, "stage block", true);
        assert!(text.ends_with("End your turn after the confirmation.\n"));
    }
}
