pub mod context;
pub mod criteria;
pub mod gates;
pub mod transitions;

pub use context::CriteriaContext;
pub use criteria::{
    run_acceptance, run_acceptance_with_config, run_single_criterion,
    run_single_criterion_with_timeout, AcceptanceResult, CriteriaConfig, CriterionResult,
    DEFAULT_COMMAND_TIMEOUT,
};
pub use gates::{human_gate, GateConfig, GateDecision};
pub use transitions::{
    list_all_stages, load_stage, save_stage, serialize_stage_to_markdown, transition_stage,
    trigger_dependents,
};
