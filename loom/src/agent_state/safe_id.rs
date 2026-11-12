//! The id shape every transcript-, receipt- and path-bound id must have.

/// Whether `value` is a safe id: 1 to 128 ASCII alphanumerics, `.`, `_` and `-`, starting with an
/// alphanumeric. Such an id cannot climb out of a directory or carry control characters, so it can
/// name a file and be printed unescaped. It lives here, a module that imports nothing from its
/// callers, because `background_task_id` and the forward receipts both apply it.
pub fn is_safe_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=128).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
#[path = "safe_id_tests.rs"]
mod safe_id_tests;
