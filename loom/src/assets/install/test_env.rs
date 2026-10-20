//! Test-only isolation of the install-root resolver ([`super::default_paths`]).
//!
//! The resolver reads `HOME`, and `find_hooks_dir` reads `LOOM_HOOKS_DIR`,
//! which the developer's shell may export, so a test that depends on the
//! resolved roots pins both. Mutating the process environment is only sound
//! under `#[serial]`.

use std::ffi::OsString;
use std::path::Path;

/// Restores every variable it touched to its earlier value (or absence) on
/// drop, a panic included.
pub(crate) struct InstallRootsEnv {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl InstallRootsEnv {
    /// `HOME` set to `home` and `LOOM_HOOKS_DIR` unset, so every root
    /// resolves to its `<home>/.claude` / `<home>/.codex` default.
    pub(crate) fn isolate(home: &Path) -> Self {
        let mut env = Self { saved: Vec::new() };
        env.set("HOME", home);
        env.save("LOOM_HOOKS_DIR");
        std::env::remove_var("LOOM_HOOKS_DIR");
        env
    }

    /// Set `key` to `value` for the guard's lifetime.
    pub(crate) fn set(&mut self, key: &'static str, value: impl AsRef<std::ffi::OsStr>) {
        self.save(key);
        std::env::set_var(key, value);
    }

    /// Record `key`'s value before its first change; later changes keep it.
    fn save(&mut self, key: &'static str) {
        if !self.saved.iter().any(|(saved, _)| *saved == key) {
            self.saved.push((key, std::env::var_os(key)));
        }
    }
}

impl Drop for InstallRootsEnv {
    fn drop(&mut self) {
        for (key, original) in self.saved.drain(..).rev() {
            match original {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
