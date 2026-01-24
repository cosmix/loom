use crate::models::stage::Stage;

/// Print failure guidance to the user after a stage fails
pub fn print_failure_guidance(stage: &Stage) {
    let failure_type = stage
        .failure_info
        .as_ref()
        .map(|i| format!("{:?}", i.failure_type))
        .unwrap_or_else(|| "Unknown".to_string());

    let max_retries = stage.max_retries.unwrap_or(3);

    eprintln!();
    eprintln!("┌──────────────────────────────────────────────────────────────┐");
    eprintln!("│  STAGE FAILED: {:<46} │", truncate(&stage.name, 46));
    eprintln!("│                                                              │");
    eprintln!(
        "│  Type: {:<14}  Attempts: {}/{:<24} │",
        truncate(&failure_type, 14),
        stage.retry_count,
        format!("{}", max_retries)
    );
    eprintln!(
        "│  Reason: {:<52} │",
        truncate(stage.close_reason.as_deref().unwrap_or("Unknown"), 52)
    );
    eprintln!("└──────────────────────────────────────────────────────────────┘");
    eprintln!();
    eprintln!("Options:");
    eprintln!(
        "  loom diagnose {}       Analyze failure with session",
        stage.id
    );
    eprintln!(
        "  loom stage retry {}    Reset and retry (use --force to ignore limit)",
        stage.id
    );
    eprintln!(
        "  loom stage skip {}     Skip stage (dependents will be blocked)",
        stage.id
    );
    eprintln!(
        "  loom stage reset {}    Reset retry count and re-queue",
        stage.id
    );
    eprintln!();
}

/// Truncate string to max characters (UTF-8 safe)
fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else {
        format!(
            "{}...",
            s.chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world", 8);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn test_truncate_utf8_emoji() {
        // Emoji are 4 bytes each, 8 emoji = 8 chars (but 32 bytes)
        let input = "🎉🎊🎁🎈🎂🎄🎅🎆";
        let result = truncate(input, 6);
        // 6 chars max, minus 3 for "..." = 3 emoji
        assert_eq!(result, "🎉🎊🎁...");
    }

    #[test]
    fn test_truncate_utf8_cjk() {
        // CJK characters, 8 chars (24 bytes)
        let input = "你好世界测试安全";
        let result = truncate(input, 6);
        assert_eq!(result, "你好世...");
    }

    #[test]
    fn test_truncate_very_short_max() {
        // Edge case: max_len smaller than 3 (the "..." suffix)
        let result = truncate("hello", 2);
        assert_eq!(result, "...");
    }
}
