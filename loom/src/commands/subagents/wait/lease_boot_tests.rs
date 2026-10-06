use super::lease::{BootClock, SystemBootClock};
use crate::process::boot_id::BOOT_ID_ENV;
use serial_test::serial;

/// Pins an environment variable for a test's duration and restores whatever
/// value (or absence) it had beforehand on drop.
struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }

    fn unset(key: &'static str) -> Self {
        let original = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
#[serial]
fn a_valid_session_boot_id_wins_over_the_os_source() {
    let _guard = EnvVarGuard::set(BOOT_ID_ENV, "0f8fad5b-d9cb-469f-a165-70867728950e");

    assert_eq!(
        SystemBootClock.boot_id().unwrap(),
        "0f8fad5b-d9cb-469f-a165-70867728950e"
    );
}

#[test]
#[serial]
fn a_malformed_session_boot_id_is_ignored() {
    let _guard = EnvVarGuard::set(BOOT_ID_ENV, "not-a-uuid");

    if let Ok(value) = SystemBootClock.boot_id() {
        assert_ne!(value, "not-a-uuid");
    }
}

#[test]
#[serial]
fn an_unset_session_boot_id_uses_the_os_source() {
    let _guard = EnvVarGuard::unset(BOOT_ID_ENV);

    let value = SystemBootClock.boot_id();

    #[cfg(target_os = "linux")]
    assert_eq!(
        value.unwrap(),
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .unwrap()
            .trim()
    );
    #[cfg(not(target_os = "linux"))]
    let _ = value;
}
