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

pub(super) const HELP_TEMPLATE: &str = "
   ╷
   │  ┌─┐┌─┐┌┬┐
   │  │ ││ ││││
   ┴─┘└─┘└─┘┴ ┴

{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}";

pub(super) fn positive_usize(value: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("'{value}' is not a valid positive integer"))?;
    if parsed == 0 {
        return Err("value must be at least 1".to_string());
    }
    Ok(parsed)
}
