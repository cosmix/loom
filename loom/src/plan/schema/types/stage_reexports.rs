//! Re-exports of stage-related types canonically defined elsewhere.
//!
//! Split out of `types.rs` to keep it under its maintainability line-count
//! baseline; these are re-exports only, no new type definitions.

/// Agent lanes a stage may delegate implementation work to.
///
/// Re-exported so `StageDefinition` and the runtime `Stage` name the same types.
/// The canonical definitions are in crate::models::stage.
pub use crate::models::stage::{Implementer, Implementers};

/// Wiring check to verify component connections.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::WiringCheck.
pub use crate::models::stage::WiringCheck;

/// Enhanced truth check with extended success criteria.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::TruthCheck.
pub use crate::models::stage::TruthCheck;

/// Unified acceptance criterion - either a simple shell command or an extended check.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::AcceptanceCriterion.
pub use crate::models::stage::AcceptanceCriterion;

/// Success criteria for wiring tests.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::SuccessCriteria.
pub use crate::models::stage::SuccessCriteria;

/// Wiring test to verify component integration.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::WiringTest.
pub use crate::models::stage::WiringTest;

/// Configuration for dead code detection.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::DeadCodeCheck.
pub use crate::models::stage::DeadCodeCheck;

/// Regression test requirement for bug-fix stages.
///
/// Re-exported from models::stage for backward compatibility.
/// The canonical definition is in crate::models::stage::RegressionTest.
pub use crate::models::stage::RegressionTest;

/// Contract and reachability specs a `version: 2` stage declares.
/// The canonical definitions are in `types_v2.rs`.
pub use crate::plan::schema::types_v2::{ContractSpec, ReachableCheck};
