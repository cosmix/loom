//! The loom executable a boundary starts as its trusted bridge
//! (`worker-exec`).
use std::path::PathBuf;

/// This process's own executable, canonical: loom in the daemon and the CLI.
/// Under `cargo test` that is the libtest harness, which answers no
/// `worker-exec`, so a test that runs a production launch sets
/// [`TestOverride`] first.
pub(crate) fn current() -> std::io::Result<PathBuf> {
    #[cfg(test)]
    if let Some(path) = overridden() {
        return Ok(path);
    }
    std::env::current_exe().and_then(|path| path.canonicalize())
}

#[cfg(test)]
static OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// The program a [`TestOverride`] set, if one is in scope.
#[cfg(test)]
pub(crate) fn overridden() -> Option<PathBuf> {
    OVERRIDE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Makes [`current`] answer a given program until dropped, then restores
/// what was set before. Process-wide: callers are `#[serial_test::serial]`.
#[cfg(all(test, target_os = "macos"))]
#[must_use = "the override ends when the guard drops"]
pub(crate) struct TestOverride(Option<PathBuf>);

#[cfg(all(test, target_os = "macos"))]
impl TestOverride {
    pub(crate) fn set(program: PathBuf) -> Self {
        let mut slot = OVERRIDE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Self(slot.replace(program))
    }
}

#[cfg(all(test, target_os = "macos"))]
impl Drop for TestOverride {
    fn drop(&mut self) {
        *OVERRIDE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = self.0.take();
    }
}

/// The loom binary cargo built beside the test harness
/// (`target/<profile>/loom`), when there is one.
#[cfg(all(test, target_os = "macos"))]
pub(crate) fn built_loom() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let loom = exe.parent()?.parent()?.join("loom");
    loom.is_file().then_some(loom)
}
