//! Stage output commands
//!
//! Allows stages to emit structured outputs that can be consumed by dependent stages.

use anyhow::{bail, Result};
use serde_json::Value;
use std::path::Path;

use crate::models::stage::StageOutput;
use crate::verify::transitions::{load_stage, save_stage};

/// Set an output for a stage.
///
/// The value is parsed as JSON if it looks like JSON, otherwise treated as a string.
/// This allows simple values like "true" or "42" to work as expected, while also
/// supporting complex values like `{"key": "value"}` or `["a", "b", "c"]`.
///
/// # Arguments
/// * `stage_id` - The ID of the stage to set the output on
/// * `key` - The output key (must be unique within the stage)
/// * `value` - The output value (JSON or plain string)
/// * `description` - Optional description of the output
pub fn set(
    stage_id: String,
    key: String,
    value: String,
    description: Option<String>,
) -> Result<()> {
    let work_dir = Path::new(".work");

    // Validate key format (alphanumeric, underscores, dashes)
    if !key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        bail!("Output key must contain only alphanumeric characters, underscores, and dashes");
    }

    if key.is_empty() || key.len() > 64 {
        bail!("Output key must be 1-64 characters");
    }

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Parse value as JSON if possible, otherwise use as string
    let json_value = parse_value(&value);

    let output = StageOutput {
        key: key.clone(),
        value: json_value.clone(),
        description: description.unwrap_or_else(|| format!("Output: {key}")),
    };

    let was_new = stage.set_output(output);
    save_stage(&stage, work_dir)?;

    let action = if was_new { "added" } else { "updated" };
    println!("Output '{key}' {action} for stage '{stage_id}'");
    println!("  Value: {}", format_value(&json_value));

    Ok(())
}

/// List all outputs for a stage.
pub fn list(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let stage = load_stage(&stage_id, work_dir)?;

    if stage.outputs.is_empty() {
        println!("No outputs for stage '{stage_id}'");
        return Ok(());
    }

    println!("Outputs for stage '{stage_id}':");
    println!();

    for output in &stage.outputs {
        println!("  {}:", output.key);
        println!("    Value: {}", format_value(&output.value));
        println!("    Description: {}", output.description);
        println!();
    }

    Ok(())
}

/// Get a specific output value.
pub fn get(stage_id: String, key: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let stage = load_stage(&stage_id, work_dir)?;

    match stage.get_output(&key) {
        Some(output) => {
            // Output just the value for scripting use
            println!("{}", format_value(&output.value));
            Ok(())
        }
        None => {
            bail!("Output '{}' not found for stage '{}'", key, stage_id)
        }
    }
}

/// Remove an output from a stage.
pub fn remove(stage_id: String, key: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if stage.remove_output(&key) {
        save_stage(&stage, work_dir)?;
        println!("Output '{key}' removed from stage '{stage_id}'");
        Ok(())
    } else {
        bail!("Output '{}' not found for stage '{}'", key, stage_id)
    }
}

/// Parse a string value into JSON Value.
///
/// Tries to parse as JSON first. If that fails, returns the string as a JSON string value.
fn parse_value(value: &str) -> Value {
    // Try to parse as JSON
    if let Ok(json_value) = serde_json::from_str(value) {
        return json_value;
    }

    // Handle special string values
    let trimmed = value.trim();
    if trimmed == "true" {
        return Value::Bool(true);
    }
    if trimmed == "false" {
        return Value::Bool(false);
    }
    if trimmed == "null" {
        return Value::Null;
    }

    // Try to parse as a number
    if let Ok(n) = trimmed.parse::<i64>() {
        return Value::Number(n.into());
    }
    if let Ok(n) = trimmed.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }

    // Default to string
    Value::String(value.to_string())
}

/// Format a JSON value for display.
fn format_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{}\"", s),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_value_string() {
        assert_eq!(parse_value("hello"), Value::String("hello".to_string()));
    }

    #[test]
    fn test_parse_value_bool() {
        assert_eq!(parse_value("true"), Value::Bool(true));
        assert_eq!(parse_value("false"), Value::Bool(false));
    }

    #[test]
    fn test_parse_value_number() {
        assert_eq!(parse_value("42"), Value::Number(42.into()));
        assert_eq!(parse_value("-10"), Value::Number((-10).into()));
    }

    #[test]
    fn test_parse_value_json_object() {
        let result = parse_value(r#"{"key": "value"}"#);
        assert!(result.is_object());
        assert_eq!(result.get("key"), Some(&Value::String("value".to_string())));
    }

    #[test]
    fn test_parse_value_json_array() {
        let result = parse_value(r#"["a", "b", "c"]"#);
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_parse_value_null() {
        assert_eq!(parse_value("null"), Value::Null);
    }

    #[test]
    fn test_format_value_string() {
        assert_eq!(
            format_value(&Value::String("hello".to_string())),
            "\"hello\""
        );
    }

    #[test]
    fn test_format_value_number() {
        assert_eq!(format_value(&Value::Number(42.into())), "42");
    }

    #[test]
    fn test_format_value_bool() {
        assert_eq!(format_value(&Value::Bool(true)), "true");
    }
}
