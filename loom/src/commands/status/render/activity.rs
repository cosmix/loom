//! Activity status rendering helpers

/// One-line explanation for an `Orphaned` stage: it claims to be executing
/// but no session record exists for it at all, which is a different problem
/// from a session that has merely gone quiet.
/// Names the two ways out so the operator does not have to look them up.
pub fn render_orphaned_warning(stage_id: &str) -> String {
    format!(
        "  Stage '{stage_id}' claims Executing with no session record - run \
         `loom repair` or `loom stage reset --kill-session {stage_id}`"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orphaned_warning_names_the_stage_and_both_ways_out() {
        let message = render_orphaned_warning("my-stage");
        assert!(message.contains("my-stage"));
        assert!(message.contains("loom repair"));
        assert!(message.contains("loom stage reset --kill-session my-stage"));
    }
}
