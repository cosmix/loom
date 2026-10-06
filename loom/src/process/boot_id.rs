//! Resolves the current boot identity from the environment or operating system.

#[cfg(any(target_os = "linux", target_os = "macos"))]
use anyhow::Context;
use anyhow::{bail, Result};

/// Environment variable supplied by the daemon.
pub const BOOT_ID_ENV: &str = "LOOM_BOOT_ID";

fn is_uuid_shaped(value: &str) -> bool {
    let lengths = [8, 4, 4, 4, 12];
    let mut groups = value.split("-");

    for length in lengths {
        let Some(group) = groups.next() else {
            return false;
        };
        if group.len() != length || !group.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return false;
        }
    }

    groups.next().is_none()
}

/// Resolves a boot identity from an environment value or operating system source.
pub fn resolve_boot_id(
    env_value: Option<&str>,
    os_boot_id: impl FnOnce() -> Result<String>,
) -> Result<String> {
    if let Some(value) = env_value
        .map(str::trim)
        .filter(|value| is_uuid_shaped(value))
    {
        return Ok(value.to_owned());
    }

    os_boot_id()
}

/// Reads the current boot identity from the operating system.
#[cfg(target_os = "linux")]
pub fn os_boot_id() -> Result<String> {
    let value = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .context("failed to read Linux boot ID")?;
    let value = value.trim();
    if value.is_empty() {
        bail!("Linux boot ID is empty");
    }
    Ok(value.to_owned())
}

/// Reads the current boot identity from the operating system.
#[cfg(target_os = "macos")]
pub fn os_boot_id() -> Result<String> {
    macos_boot_id()
}

/// Reads the current boot identity from the operating system.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn os_boot_id() -> Result<String> {
    bail!("boot identity is unsupported on this operating system")
}

#[cfg(target_os = "macos")]
fn macos_boot_id() -> Result<String> {
    let name = b"kern.bootsessionuuid\0";
    let mut size = 0usize;
    // SAFETY: the name is NUL-terminated and the null output pointer requests the required size.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error()).context("failed to size macOS boot ID");
    }

    let mut bytes = vec![0u8; size];
    // SAFETY: `bytes` provides `size` writable bytes and the name remains NUL-terminated.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error()).context("failed to read macOS boot ID");
    }

    bytes.truncate(size);
    let value = std::str::from_utf8(&bytes)?
        .trim_matches(|c: char| c == char::from(0) || c.is_whitespace());
    if value.is_empty() {
        bail!("macOS boot ID is empty");
    }
    Ok(value.to_owned())
}

/// Resolves the current boot identity.
pub fn current_boot_id() -> Result<String> {
    resolve_boot_id(std::env::var(BOOT_ID_ENV).ok().as_deref(), os_boot_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn valid_env_wins_without_calling_the_closure() {
        let input = "0f8fad5b-d9cb-469f-a165-70867728950e";
        let called = Cell::new(false);
        let result = resolve_boot_id(Some(input), || {
            called.set(true);
            Ok("from-os".to_owned())
        })
        .unwrap();

        assert_eq!(result, input);
        assert!(!called.get());
    }

    #[test]
    fn env_value_is_trimmed_before_the_shape_check() {
        let input = " 0f8fad5b-d9cb-469f-a165-70867728950e\n";
        let result = resolve_boot_id(Some(input), || Ok("from-os".to_owned())).unwrap();

        assert_eq!(result, "0f8fad5b-d9cb-469f-a165-70867728950e");
    }

    #[test]
    fn mixed_case_uuid_is_accepted() {
        let input = "0F8fAD5b-d9CB-469f-A165-70867728950E";
        let result = resolve_boot_id(Some(input), || Ok("from-os".to_owned())).unwrap();

        assert_eq!(result, input);
    }

    #[test]
    fn empty_env_falls_back_to_the_os_source() {
        let empty = resolve_boot_id(Some(""), || Ok("from-os".to_owned())).unwrap();
        let whitespace = resolve_boot_id(Some("   "), || Ok("from-os".to_owned())).unwrap();

        assert_eq!(empty, "from-os");
        assert_eq!(whitespace, "from-os");
    }

    #[test]
    fn non_uuid_env_falls_back_to_the_os_source() {
        let invalid_text =
            resolve_boot_id(Some("not-a-uuid"), || Ok("from-os".to_owned())).unwrap();
        let invalid_dashes = resolve_boot_id(Some("0f8fad5b-d9cb4-69f-a165-70867728950e"), || {
            Ok("from-os".to_owned())
        })
        .unwrap();
        let invalid_hex = resolve_boot_id(Some("0f8fad5b-d9cb-469f-a165-70867728950g"), || {
            Ok("from-os".to_owned())
        })
        .unwrap();

        assert_eq!(invalid_text, "from-os");
        assert_eq!(invalid_dashes, "from-os");
        assert_eq!(invalid_hex, "from-os");
    }

    #[test]
    fn missing_env_falls_back_to_the_os_source() {
        let result = resolve_boot_id(None, || Ok("from-os".to_owned())).unwrap();

        assert_eq!(result, "from-os");
    }

    #[test]
    fn os_error_with_no_env_propagates() {
        let error = resolve_boot_id(None, || Err(anyhow::anyhow!("os source denied"))).unwrap_err();

        assert!(error.to_string().contains("os source denied"));
    }

    #[test]
    fn os_error_with_an_invalid_env_propagates() {
        let error = resolve_boot_id(Some("garbage"), || Err(anyhow::anyhow!("os source denied")))
            .unwrap_err();

        assert!(error.to_string().contains("os source denied"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_os_boot_id_is_uuid_shaped() {
        assert!(is_uuid_shaped(&os_boot_id().unwrap()));
    }
}
