//! Plan YAML schema definitions and validation

mod detect;
mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use detect::detect_stage_type;
pub use types::{
    AcceptanceCriterion, AdjudicationConfig, ChangeImpactConfig, ChangeImpactPolicy, DeadCodeCheck,
    FilesystemConfig, LinuxConfig, LoomConfig, LoomMetadata, NetworkConfig, PermissionMode,
    RegressionTest, SandboxConfig, StageDefinition, StageSandboxConfig, StageType, SuccessCriteria,
    TruthCheck, ValidationError, WiringCheck, WiringTest, ALLOWED_REASONING_EFFORTS,
};
pub use validation::{
    check_knowledge_recommendations, check_sandbox_recommendations, validate,
    validate_structural_preflight,
};
