//! Plan YAML schema definitions and validation

mod types;
mod validation;

#[cfg(test)]
mod tests;

pub use types::{
    FilesystemConfig, LinuxConfig, LoomConfig, LoomMetadata, NetworkConfig, SandboxConfig,
    StageDefinition, StageSandboxConfig, StageType, ValidationError, WiringCheck,
};
pub use validation::{
    check_code_review_recommendations, check_knowledge_recommendations, check_sandbox_recommendations,
    validate,
};
