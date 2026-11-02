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

#[cfg(test)]
mod tests {
    use super::normalize_model;

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
