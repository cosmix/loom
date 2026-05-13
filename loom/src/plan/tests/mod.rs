//! Plan-level integration tests.
//!
//! Module path: `plan::tests::*`. Tests that exercise multi-component
//! plan flows (parser + schema + amendment) belong here. Per-module unit
//! tests stay inline as `#[cfg(test)] mod tests {}` inside the module.

pub mod amendment;
