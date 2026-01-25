pub mod context;
pub mod criteria;
pub mod gates;
pub mod goal_backward;
pub mod task_verification;
pub mod transitions;

pub use context::CriteriaContext;
pub use criteria::{
    run_acceptance, run_acceptance_with_config, run_single_criterion,
    run_single_criterion_with_timeout, AcceptanceResult, CriteriaConfig, CriterionResult,
    DEFAULT_COMMAND_TIMEOUT,
};
pub use gates::{human_gate, GateConfig, GateDecision};
pub use goal_backward::{
    run_goal_backward_verification, GapType, GoalBackwardResult, VerificationGap,
};
pub use task_verification::{
    run_single_verification, run_task_verifications, summarize_verifications,
    DEFAULT_VERIFICATION_TIMEOUT,
};
pub use transitions::{
    list_all_stages, load_stage, save_stage, serialize_stage_to_markdown, transition_stage,
    trigger_dependents,
};
