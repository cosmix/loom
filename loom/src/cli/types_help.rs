//! Presentation-only constants and helpers shared by `types.rs`.

/// Rendered by `-v`/`--version`: version, commit, build date, target triple.
pub(super) const VERSION_STRING: &str = concat!(
    env!("LOOM_VERSION"),
    " (",
    env!("LOOM_COMMIT"),
    ", ",
    env!("LOOM_BUILD_DATE"),
    ", ",
    env!("LOOM_TARGET"),
    ")"
);

pub(super) fn help_template() -> String {
    format!(
        "\n{}\n\n{{about-with-newline}}\n{{usage-heading}} {{usage}}\n\n{{all-args}}{{after-help}}",
        crate::LOGO
    )
}

pub(super) fn positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("'{value}' is not a valid positive integer"))?;
    if parsed == 0 {
        return Err("value must be at least 1".to_string());
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use crate::cli::types::Cli;
    use clap::CommandFactory;

    #[test]
    fn help_uses_shared_logo() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.starts_with(&format!("{}\n\n", crate::LOGO)), "{help}");
        assert!(help.contains("Usage:"), "{help}");
    }
}
