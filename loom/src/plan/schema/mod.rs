//! Plan YAML schema definitions and validation

mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use types::{
    ChangeImpactConfig, ChangeImpactPolicy, DeadCodeCheck, FilesystemConfig, LinuxConfig,
    LoomConfig, LoomMetadata, NetworkConfig, RegressionTest, SandboxConfig, StageDefinition,
    StageSandboxConfig, StageType, SuccessCriteria, TruthCheck, ValidationError, WiringCheck,
    WiringTest,
};
pub use validation::{
    check_knowledge_recommendations, check_sandbox_recommendations, validate,
    validate_structural_preflight,
};
