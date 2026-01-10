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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
