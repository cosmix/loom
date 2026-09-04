//! Claude.ai usage polling: HTTP client, request, and response parsing.

use crate::quota::model::{self, ProviderQuota, QuotaWindow, WindowKind};
use anyhow::{anyhow, Context, Result};
use std::fmt;
use std::io::Read;
use std::time::Duration;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_BODY_BYTES: u64 = 64 * 1024;

/// Typed 429 response, so the poller can back off using the server's own
/// `Retry-After` rather than guessing.
#[derive(Debug)]
pub struct RateLimited {
    pub retry_after_secs: Option<u64>,
}

impl fmt::Display for RateLimited {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rate limited")
    }
}

impl std::error::Error for RateLimited {}

/// Build the HTTP client used to poll the usage endpoint.
///
/// Mirrors `commands::self_update::client::create_http_client`'s security
/// posture (HTTPS-only, bounded connect/total timeouts) with tighter timeouts
/// appropriate for a background poll rather than a large download.
pub fn build_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("loom/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build the claude usage http client")
}

/// Fetch and parse the caller's current usage. `token` is used for exactly
/// one request and is never included in the returned error.
pub fn fetch(client: &reqwest::blocking::Client, token: &str, now: i64) -> Result<ProviderQuota> {
    let response = client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .context("failed to reach the claude usage endpoint")?;

    if response.status().as_u16() == 429 {
        let retry_after_secs = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok());
        return Err(anyhow::Error::new(RateLimited { retry_after_secs }));
    }
    if !response.status().is_success() {
        return Err(anyhow!("HTTP {}", response.status().as_u16()));
    }

    let mut body = String::new();
    response
        .take(MAX_BODY_BYTES)
        .read_to_string(&mut body)
        .context("failed to read the claude usage response body")?;

    parse_response(&body, now)
}

/// Parse a usage response body into a [`ProviderQuota`].
///
/// Accepts either the `{"five_hour": ..., "seven_day": ...}` bucket shape or
/// the `{"limits": [...]}` array shape; buckets win when both are present. A
/// body that is valid JSON but matches neither shape yields zero windows
/// rather than an error.
pub fn parse_response(body: &str, now: i64) -> Result<ProviderQuota> {
    let value: serde_json::Value =
        serde_json::from_str(body).context("invalid claude usage response")?;

    let windows = parse_buckets(&value).unwrap_or_else(|| parse_limits(&value));

    Ok(ProviderQuota {
        observed_at: now,
        windows,
        plan: None,
        error: None,
    })
}

/// `{"five_hour": {"utilization": .., "resets_at": ..}, "seven_day": {...}}`.
///
/// `None` when neither key is present at all, signalling "not this shape" so
/// the caller falls back to [`parse_limits`]. A key present but `null` means
/// no window for that bucket, not "try the other shape".
fn parse_buckets(value: &serde_json::Value) -> Option<Vec<QuotaWindow>> {
    let five_hour = value.get("five_hour");
    let seven_day = value.get("seven_day");
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }

    let mut windows = Vec::new();
    if let Some(window) = bucket_window(five_hour, WindowKind::FiveHour) {
        windows.push(window);
    }
    if let Some(window) = bucket_window(seven_day, WindowKind::SevenDay) {
        windows.push(window);
    }
    Some(windows)
}

fn bucket_window(bucket: Option<&serde_json::Value>, kind: WindowKind) -> Option<QuotaWindow> {
    let bucket = bucket.filter(|value| !value.is_null())?;
    let used_percent = model::clamp_percent(bucket.get("utilization").and_then(|v| v.as_f64()))?;
    let resets_at = parse_resets_at(bucket.get("resets_at"));
    Some(QuotaWindow {
        kind,
        used_percent,
        resets_at,
    })
}

/// `{"limits": [{"kind": "session"|"weekly_all", "percent": .., "resets_at": ..}]}`.
///
/// Unknown `kind`s are ignored. A limit reporting `percent: 0` with no
/// `resets_at` is a provider-side placeholder for "not applicable" and is
/// ignored rather than rendered as 0% used.
fn parse_limits(value: &serde_json::Value) -> Vec<QuotaWindow> {
    let mut five_hour: Option<QuotaWindow> = None;
    let mut seven_day: Option<QuotaWindow> = None;

    let Some(limits) = value.get("limits").and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    for limit in limits {
        let kind = match limit.get("kind").and_then(|v| v.as_str()) {
            Some("session") => WindowKind::FiveHour,
            Some("weekly_all") => WindowKind::SevenDay,
            _ => continue,
        };

        let raw_percent = limit.get("percent").and_then(|v| v.as_f64());
        let resets_at_value = limit.get("resets_at");
        let is_placeholder =
            raw_percent == Some(0.0) && resets_at_value.is_none_or(|v| v.is_null());
        if is_placeholder {
            continue;
        }

        let Some(used_percent) = model::clamp_percent(raw_percent) else {
            continue;
        };
        let resets_at = parse_resets_at(resets_at_value);
        let window = QuotaWindow {
            kind,
            used_percent,
            resets_at,
        };
        match kind {
            WindowKind::FiveHour => five_hour.get_or_insert(window),
            WindowKind::SevenDay => seven_day.get_or_insert(window),
        };
    }

    [five_hour, seven_day].into_iter().flatten().collect()
}

/// Parse a `resets_at` value that may be an RFC 3339 timestamp (any offset,
/// fractional seconds allowed), an epoch-seconds/-milliseconds number, or
/// absent/null/unparsable - the last three all become `None` rather than an
/// error, since a bad reset time should never hide a window's percentage.
fn parse_resets_at(value: Option<&serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::String(text) => chrono::DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|dt| dt.timestamp()),
        serde_json::Value::Number(number) => {
            number.as_f64().map(|n| model::normalize_epoch(n as i64))
        }
        _ => None,
    }
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
