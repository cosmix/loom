//! Contract: every stage prefix (standard, integration-verify, knowledge-distill) states the
//! refused-commit rule, and the standard prefix stays within its fixed ceiling.

use super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_stable_prefix,
};

const RULE_PHRASES: [&str; 2] = [
    "A refused commit never stops a filing",
    "it outranks a plan instruction",
];

/// The standard stable prefix's ceiling, written out so that raising the ceiling elsewhere
/// cannot make room for the rule.
const STANDARD_PREFIX_CEILING: usize = 8192;

#[test]
fn every_stage_prefix_states_the_refused_commit_rule() {
    let standard = generate_stable_prefix();
    let prefixes = [
        ("standard", standard.as_str()),
        (
            "integration-verify",
            &generate_integration_verify_stable_prefix(),
        ),
        (
            "knowledge-distill",
            &generate_knowledge_distill_stable_prefix(),
        ),
    ];
    for (name, prefix) in prefixes {
        for phrase in RULE_PHRASES {
            assert!(
                prefix.contains(phrase),
                "the {name} stable prefix lacks {phrase:?}"
            );
        }
    }
    assert!(
        standard.len() <= STANDARD_PREFIX_CEILING,
        "the standard stable prefix is {} bytes, over its ceiling of {STANDARD_PREFIX_CEILING}",
        standard.len()
    );
}
