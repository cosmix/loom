use super::blocks::RETIRED_PHRASES;
use super::*;

#[test]
fn no_guidance_surface_still_tells_a_subagent_to_verify() {
    let mut surfaces = guidance_surfaces();
    surfaces.push(("signal stable prefix".to_string(), generate_stable_prefix()));
    surfaces.push((
        "signal integration-verify prefix".to_string(),
        generate_integration_verify_stable_prefix(),
    ));
    surfaces.push((
        "signal knowledge prefix".to_string(),
        generate_knowledge_stable_prefix(),
    ));
    surfaces.push((
        "signal knowledge-distill prefix".to_string(),
        generate_knowledge_distill_stable_prefix(),
    ));

    let mut leftovers = Vec::new();
    for (label, text) in surfaces {
        for phrase in RETIRED_PHRASES {
            if text.contains(phrase) {
                leftovers.push(format!("{label} still contains {phrase:?}"));
            }
        }
    }

    assert!(
        leftovers.is_empty(),
        "guidance surfaces still carry phrasing the no-verify rule retired, so a \
         subagent obeying them would be blocked by loom-hooks/subagent-verify-guard.sh \
         with no allowed alternative:\n  {}",
        leftovers.join("\n  ")
    );
}
