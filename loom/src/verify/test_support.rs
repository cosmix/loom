/// The isolated harness uses an empty Rustup home. This fixture grants the
/// installed compiler tree read access through the normal confinement input.
/// Tests holding this guard must share the default `serial_test` lock.
pub(crate) struct ToolchainReadRoot(Option<Option<std::ffi::OsString>>);

impl ToolchainReadRoot {
    pub(crate) fn from_runner() -> Self {
        let saved = std::env::var_os("LOOM_TEST_TOOLCHAIN").map(|toolchain| {
            let previous = std::env::var_os("RUSTUP_HOME");
            std::env::set_var("RUSTUP_HOME", toolchain);
            previous
        });
        Self(saved)
    }
}

impl Drop for ToolchainReadRoot {
    fn drop(&mut self) {
        match &self.0 {
            Some(Some(previous)) => std::env::set_var("RUSTUP_HOME", previous),
            Some(None) => std::env::remove_var("RUSTUP_HOME"),
            None => {}
        }
    }
}
