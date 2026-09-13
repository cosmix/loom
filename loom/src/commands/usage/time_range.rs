use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

/// Inclusive event-time bounds for every usage source.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TimeRange {
    pub(crate) since: DateTime<Utc>,
    pub(crate) until: Option<DateTime<Utc>>,
}

impl TimeRange {
    pub(crate) fn parse(since: &str, until: Option<&str>, now: DateTime<Utc>) -> Result<Self> {
        let since = parse_since_at(since, now)?;
        let until = until.map(parse_until).transpose()?;
        if until.is_some_and(|bound| bound < since) {
            bail!("--until must be greater than or equal to --since");
        }
        Ok(Self { since, until })
    }

    pub(crate) fn includes(&self, timestamp: DateTime<Utc>) -> bool {
        timestamp >= self.since && self.until.is_none_or(|until| timestamp <= until)
    }
}

pub(crate) fn parse_since_at(spec: &str, now: DateTime<Utc>) -> Result<DateTime<Utc>> {
    if let Ok(date) = NaiveDate::parse_from_str(spec, "%Y-%m-%d") {
        let midnight = date
            .and_hms_opt(0, 0, 0)
            .context("ISO date has no midnight")?;
        return Ok(Utc.from_utc_datetime(&midnight));
    }
    now.checked_sub_signed(duration_spec(spec)?)
        .context("--since duration is too large")
}

fn parse_until(spec: &str) -> Result<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(spec).with_context(|| {
        format!("Invalid --until value: {spec}; expected an offset-aware RFC3339 instant")
    })?;
    Ok(parsed.with_timezone(&Utc))
}

fn duration_spec(spec: &str) -> Result<Duration> {
    let Some(unit) = spec.chars().next_back() else {
        bail!("Invalid --since value: {spec}")
    };
    let number = spec
        .strip_suffix(unit)
        .unwrap_or_default()
        .parse::<i64>()
        .with_context(|| format!("Invalid --since value: {spec}"))?;
    if number < 0 {
        bail!("Invalid --since value: {spec}")
    }
    let hours = match unit {
        'm' => return Duration::try_minutes(number).context("--since duration is too large"),
        'h' => number,
        'd' => number
            .checked_mul(24)
            .context("--since duration is too large")?,
        _ => bail!("Invalid --since value: {spec}"),
    };
    Duration::try_hours(hours).context("--since duration is too large")
}

#[cfg(test)]
#[path = "time_range_tests.rs"]
mod tests;
