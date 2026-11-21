//! Re-exports [`crate::verify::environment::environment_marker`] for the
//! frozen contract test `environment_contract_tests`, which imports it through
//! this path. That is the only purpose of this module; production code uses
//! `crate::verify::environment`. Delete it with that test.

pub(super) use crate::verify::environment::environment_marker;
