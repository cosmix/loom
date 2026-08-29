/// JSON output keeps usage reports diffable without asking callers to parse
/// terminal-oriented rows.
pub fn print(report: &super::sections::Report) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}
