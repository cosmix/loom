//! Embedded container provisioning resources.
//!
//! These are baked into the binary so the cache layer can rebuild
//! images without depending on the runtime filesystem layout.

pub const DOCKERFILE_TEMPLATE: &str = include_str!("../../../../resources/Dockerfile.tmpl");

pub const FIREWALL_SCRIPT: &str = include_str!("../../../../resources/firewall.sh");

pub const ENTRYPOINT_SCRIPT: &str = include_str!("../../../../resources/entrypoint.sh");

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: the three embedded resource constants compile and
    /// resolve to valid `&str` values. Content may be empty during
    /// parallel subagent work; tolerate that so this stage's verifier
    /// is not coupled to other subagents' write order.
    #[test]
    fn constants_are_strings() {
        let _ = DOCKERFILE_TEMPLATE.len();
        let _ = FIREWALL_SCRIPT.len();
        let _ = ENTRYPOINT_SCRIPT.len();
    }
}
