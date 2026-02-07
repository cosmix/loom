pub mod baseline;
pub mod before_after;
pub mod context;
pub mod criteria;
pub mod goal_backward;
pub mod transitions;
pub mod utils;

pub use baseline::{
    baseline_exists, compare_to_baseline, ensure_baseline_captured, load_baseline, save_baseline,
    ChangeImpact, TestBaseline,
};
pub use before_after::{run_after_stage_checks, run_before_stage_checks};
pub use context::CriteriaContext;
pub use criteria::{
    run_acceptance, run_acceptance_with_config, run_single_criterion,
    run_single_criterion_with_timeout, AcceptanceResult, CriteriaConfig, CriterionResult,
    DEFAULT_COMMAND_TIMEOUT,
};
pub use goal_backward::{
    run_goal_backward_verification, GapType, GoalBackwardResult, VerificationGap,
};
pub use transitions::{
    list_all_stages, load_stage, save_stage, serialize_stage_to_markdown, transition_stage,
    trigger_dependents,
};
