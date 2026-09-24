//! One module per test runner, each exposing `pub static ADAPTER`.
//! Registering an adapter is one identifier in the `adapters!` list below.

use super::TestRunnerAdapter;

/// Declares each listed adapter module and collects its `ADAPTER` into `ALL`.
macro_rules! adapters {
    ($($adapter:ident),+ $(,)?) => {
        $(mod $adapter;)+

        /// Every registered adapter, in registration order.
        pub(super) static ALL: &[&dyn TestRunnerAdapter] = &[$(&$adapter::ADAPTER),+];
    };
}

adapters!(
    cargo_test,
    go_test,
    pytest,
    unittest,
    vitest,
    jest,
    mocha,
    bun_test,
    node_test,
    ctest,
    dart_test,
    flutter_test,
    dotnet_test,
    minitest,
    cargo_nextest,
    gradle,
    maven,
    sbt,
    rspec,
    phpunit,
    pest,
    swift_test,
    mix_test,
);
