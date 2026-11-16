//! Shared normalization for model names shown to operators.

pub(crate) fn normalize_model(model: &str) -> &str {
    let name = model.strip_prefix("claude-").unwrap_or(model);
    match name.rsplit_once('-') {
        Some((head, tail))
            if !head.is_empty() && tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) =>
        {
            head
        }
        _ => name,
    }
}

/// `model/effort` as operators read it (`opus/high`); the model alone when the effort is
/// unknown or empty.
pub(crate) fn model_with_effort(model: &str, effort: Option<&str>) -> String {
    match effort.filter(|effort| !effort.is_empty()) {
        Some(effort) => format!("{model}/{effort}"),
        None => model.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{model_with_effort, normalize_model};

    #[test]
    fn joins_model_and_effort_with_a_slash() {
        assert_eq!(model_with_effort("opus", Some("high")), "opus/high");
        assert_eq!(model_with_effort("opus", Some("")), "opus");
        assert_eq!(model_with_effort("opus", None), "opus");
    }

    #[test]
    fn normalizes_prefix_and_date_without_splitting_unicode() {
        for (input, expected) in [
            ("claude-haiku-4-5-20251001", "haiku-4-5"),
            ("claude-opus-5", "opus-5"),
            ("模型-20250101", "模型"),
            ("模型-2025abcd", "模型-2025abcd"),
            ("-20250101", "-20250101"),
        ] {
            assert_eq!(normalize_model(input), expected);
        }
    }
}
