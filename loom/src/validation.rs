//! Input validation and sanitization module for loom CLI.
//!
//! This module provides functions to validate user-supplied IDs and other inputs
//! before they are used in file path construction, preventing path traversal attacks
//! and other security issues.

use anyhow::{bail, Result};
use std::path::Path;

/// Maximum allowed length for IDs (runner, track, signal).
pub const MAX_ID_LENGTH: usize = 128;

/// Maximum allowed length for names.
pub const MAX_NAME_LENGTH: usize = 64;

/// Maximum allowed length for descriptions.
pub const MAX_DESCRIPTION_LENGTH: usize = 500;

/// Maximum allowed length for signal messages.
pub const MAX_MESSAGE_LENGTH: usize = 1000;

/// Maximum allowed length for context summaries.
pub const MAX_CONTEXT_LENGTH: usize = 5000;

/// Reserved names that cannot be used as IDs (case-insensitive).
const RESERVED_NAMES: &[&str] = &[
    ".", "..", "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Validates that an ID is safe for use in file paths.
///
/// An ID is valid if:
/// - It is not empty
/// - It is no longer than MAX_ID_LENGTH characters
/// - It contains only alphanumeric characters, dashes, and underscores
/// - It does not use reserved system names
///
/// # Arguments
///
/// * `id` - The ID string to validate
///
/// # Returns
///
/// * `Ok(())` if the ID is valid
/// * `Err` with a descriptive message if validation fails
///
/// # Examples
///
/// ```
/// use loom::validation::validate_id;
///
/// assert!(validate_id("runner-001").is_ok());
/// assert!(validate_id("track_2024").is_ok());
/// assert!(validate_id("").is_err());
/// assert!(validate_id("../etc/passwd").is_err());
/// ```
pub fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() {
        bail!("ID cannot be empty");
    }

    if id.len() > MAX_ID_LENGTH {
        bail!(
            "ID too long: {} characters (max {})",
            id.len(),
            MAX_ID_LENGTH
        );
    }

    let valid_chars = id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid_chars {
        bail!("ID '{id}' contains invalid characters. Use only alphanumeric characters, dashes (-), and underscores (_)");
    }

    let id_lower = id.to_lowercase();
    if RESERVED_NAMES.contains(&id_lower.as_str()) {
        bail!("ID '{id}' uses a reserved name");
    }

    Ok(())
}

/// Validates that a name is within acceptable length limits.
///
/// # Arguments
///
/// * `name` - The name string to validate
///
/// # Returns
///
/// * `Ok(())` if the name is valid
/// * `Err` with a descriptive message if validation fails
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Name cannot be empty");
    }

    if name.len() > MAX_NAME_LENGTH {
        bail!(
            "Name too long: {} characters (max {})",
            name.len(),
            MAX_NAME_LENGTH
        );
    }

    Ok(())
}

/// Validates that a description is within acceptable length limits.
///
/// # Arguments
///
/// * `description` - The description string to validate
///
/// # Returns
///
/// * `Ok(())` if the description is valid
/// * `Err` with a descriptive message if validation fails
pub fn validate_description(description: &str) -> Result<()> {
    if description.len() > MAX_DESCRIPTION_LENGTH {
        bail!(
            "Description too long: {} characters (max {})",
            description.len(),
            MAX_DESCRIPTION_LENGTH
        );
    }

    Ok(())
}

/// Validates that a signal message is within acceptable length limits.
///
/// # Arguments
///
/// * `message` - The message string to validate
///
/// # Returns
///
/// * `Ok(())` if the message is valid
/// * `Err` with a descriptive message if validation fails
pub fn validate_message(message: &str) -> Result<()> {
    if message.is_empty() {
        bail!("Message cannot be empty");
    }

    if message.len() > MAX_MESSAGE_LENGTH {
        bail!(
            "Message too long: {} characters (max {})",
            message.len(),
            MAX_MESSAGE_LENGTH
        );
    }

    Ok(())
}

/// Sanitizes a string for safe use as a filename component.
///
/// This function strips any directory traversal components and returns
/// only the filename portion of the input.
///
/// # Arguments
///
/// * `name` - The name to sanitize
///
/// # Returns
///
/// A sanitized string safe for use as a filename.
///
/// # Examples
///
/// ```
/// use loom::validation::safe_filename;
///
/// assert_eq!(safe_filename("runner-001"), "runner-001");
/// assert_eq!(safe_filename("../etc/passwd"), "passwd");
/// assert_eq!(safe_filename("foo/bar/baz.md"), "baz.md");
/// ```
pub fn safe_filename(name: &str) -> String {
    Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("invalid")
        .to_string()
}

/// Clap value parser for validating ID arguments.
///
/// Use this with clap's `value_parser` attribute to validate IDs at parse time.
///
/// # Examples
///
/// ```ignore
/// #[arg(value_parser = clap_id_validator)]
/// id: String,
/// ```
pub fn clap_id_validator(s: &str) -> Result<String, String> {
    validate_id(s).map_err(|e| e.to_string())?;
    Ok(s.to_string())
}

/// Clap value parser for validating name arguments.
pub fn clap_name_validator(s: &str) -> Result<String, String> {
    validate_name(s).map_err(|e| e.to_string())?;
    Ok(s.to_string())
}

/// Clap value parser for validating description arguments.
pub fn clap_description_validator(s: &str) -> Result<String, String> {
    validate_description(s).map_err(|e| e.to_string())?;
    Ok(s.to_string())
}

/// Clap value parser for validating message arguments.
pub fn clap_message_validator(s: &str) -> Result<String, String> {
    validate_message(s).map_err(|e| e.to_string())?;
    Ok(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_id_valid() {
        assert!(validate_id("runner-001").is_ok());
        assert!(validate_id("track_2024").is_ok());
        assert!(validate_id("se-001").is_ok());
        assert!(validate_id("MyRunner123").is_ok());
        assert!(validate_id("a").is_ok());
    }

    #[test]
    fn test_validate_id_empty() {
        let result = validate_id("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_id_too_long() {
        let long_id = "a".repeat(MAX_ID_LENGTH + 1);
        let result = validate_id(&long_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn test_validate_id_invalid_chars() {
        assert!(validate_id("runner/001").is_err());
        assert!(validate_id("../passwd").is_err());
        assert!(validate_id("runner 001").is_err());
        assert!(validate_id("runner.md").is_err());
        assert!(validate_id("runner:001").is_err());
    }

    #[test]
    fn test_validate_id_reserved_names() {
        assert!(validate_id(".").is_err());
        assert!(validate_id("..").is_err());
        assert!(validate_id("CON").is_err());
        assert!(validate_id("nul").is_err());
        assert!(validate_id("AUX").is_err());
    }

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_name("My Runner").is_ok());
        assert!(validate_name("Feature: Authentication").is_ok());
    }

    #[test]
    fn test_validate_name_empty() {
        assert!(validate_name("").is_err());
    }

    #[test]
    fn test_validate_name_too_long() {
        let long_name = "a".repeat(MAX_NAME_LENGTH + 1);
        assert!(validate_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_description_valid() {
        assert!(validate_description("A short description").is_ok());
        assert!(validate_description("").is_ok()); // Empty description is allowed
    }

    #[test]
    fn test_validate_description_too_long() {
        let long_desc = "a".repeat(MAX_DESCRIPTION_LENGTH + 1);
        assert!(validate_description(&long_desc).is_err());
    }

    #[test]
    fn test_validate_message_valid() {
        assert!(validate_message("Implement feature X").is_ok());
    }

    #[test]
    fn test_validate_message_empty() {
        assert!(validate_message("").is_err());
    }

    #[test]
    fn test_validate_message_too_long() {
        let long_msg = "a".repeat(MAX_MESSAGE_LENGTH + 1);
        assert!(validate_message(&long_msg).is_err());
    }

    #[test]
    fn test_safe_filename() {
        assert_eq!(safe_filename("runner-001"), "runner-001");
        assert_eq!(safe_filename("../etc/passwd"), "passwd");
        assert_eq!(safe_filename("foo/bar/baz.md"), "baz.md");
        assert_eq!(safe_filename("./current"), "current");
    }

    #[test]
    fn test_clap_validators() {
        assert!(clap_id_validator("valid-id").is_ok());
        assert!(clap_id_validator("../invalid").is_err());

        assert!(clap_name_validator("Valid Name").is_ok());
        assert!(clap_name_validator("").is_err());

        assert!(clap_description_validator("Valid description").is_ok());

        assert!(clap_message_validator("Valid message").is_ok());
        assert!(clap_message_validator("").is_err());
    }
}
