//! Synchronous HTTP client for the Anthropic Messages API.
//!
//! This intentionally does not depend on the orchestrator's async
//! infrastructure — a single dispute is rare enough that a blocking
//! POST on a dedicated worker thread (see `worker.rs`) is the simplest
//! correct option.
//!
//! Behaviour:
//! - One POST to `https://api.anthropic.com/v1/messages`.
//! - 300-second total request timeout (the Messages endpoint can take
//!   minutes on a long reasoning task).
//! - Retry policy: HTTP 429 (rate limited) and 5xx are retried up to 3
//!   times with exponential backoff (1s, 2s, 4s). Connection errors
//!   without an HTTP status are also retried.
//! - Cancellation is cooperative: between attempts (and during the
//!   backoff sleep) we observe the `cancellation` flag.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::prompt::Prompt;

/// Default model used to adjudicate disputes. Overridable via
/// `.work/config.toml::[adjudication].model`.
pub const ADJUDICATOR_MODEL: &str = "claude-opus-4-5-20250929";

/// API endpoint. Public so tests in `tests.rs` can assert it.
pub const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";

/// Total per-request timeout for the API call. The adjudicator might
/// produce a substantial reasoning trace; 300 seconds is conservative.
pub const REQUEST_TIMEOUT_SECS: u64 = 300;

/// Maximum response output token budget.
const MAX_OUTPUT_TOKENS: u32 = 4096;

/// Number of retry attempts on retriable failures. Note: this counts
/// **additional** attempts after the first try (so total HTTP calls in
/// the worst case is `RETRY_ATTEMPTS + 1`).
pub const RETRY_ATTEMPTS: u32 = 3;

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Make a synchronous Anthropic API call for the supplied prompt and
/// return the raw `content[0].text` value.
///
/// `cancellation` is polled between attempts and during backoff sleep —
/// callers should set it (e.g. on daemon shutdown) to cause the in-
/// flight HTTP request to return as soon as the underlying reqwest call
/// completes or times out.
pub fn call_anthropic(
    api_key: &str,
    prompt: &Prompt,
    cancellation: Arc<AtomicBool>,
) -> Result<String> {
    call_anthropic_with(
        api_key,
        prompt,
        cancellation,
        ADJUDICATOR_MODEL,
        ANTHROPIC_MESSAGES_URL,
    )
}

/// Variant of [`call_anthropic`] with overridable model + endpoint.
/// Used by the wiremock-backed e2e tests; production callers should use
/// the no-suffix version above.
pub fn call_anthropic_with(
    api_key: &str,
    prompt: &Prompt,
    cancellation: Arc<AtomicBool>,
    model: &str,
    endpoint: &str,
) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .context("build reqwest client")?;

    let body = serde_json::json!({
        "model": model,
        "max_tokens": MAX_OUTPUT_TOKENS,
        "system": prompt.system,
        "messages": [
            { "role": "user", "content": prompt.user }
        ],
    });

    let mut attempt: u32 = 0;
    loop {
        if cancellation.load(Ordering::Relaxed) {
            bail!("adjudicator HTTP cancelled by shutdown flag");
        }

        let send = client
            .post(endpoint)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send();

        match send {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return extract_text(resp);
                }
                if !is_retryable_status(status.as_u16()) || attempt >= RETRY_ATTEMPTS {
                    let snippet = resp
                        .text()
                        .unwrap_or_else(|_| "<unreadable body>".to_string());
                    let snippet = truncate_for_error(&snippet);
                    bail!(
                        "Anthropic API returned HTTP {} (attempt {}): {}",
                        status.as_u16(),
                        attempt + 1,
                        snippet
                    );
                }
                tracing::warn!(
                    target: "loom::adjudication::client",
                    status = status.as_u16(),
                    attempt = attempt + 1,
                    "retriable HTTP status from Anthropic API"
                );
            }
            Err(err) => {
                if attempt >= RETRY_ATTEMPTS {
                    return Err(err).context("Anthropic API request failed after retries");
                }
                tracing::warn!(
                    target: "loom::adjudication::client",
                    error = %err,
                    attempt = attempt + 1,
                    "transport error from Anthropic API",
                );
            }
        }

        attempt += 1;
        if !sleep_with_cancellation(backoff_for(attempt), &cancellation) {
            bail!("adjudicator HTTP cancelled by shutdown flag during backoff");
        }
    }
}

fn extract_text(resp: reqwest::blocking::Response) -> Result<String> {
    #[derive(Deserialize)]
    struct ApiResponse {
        content: Vec<Block>,
    }
    #[derive(Deserialize)]
    struct Block {
        #[serde(rename = "type")]
        ty: String,
        #[serde(default)]
        text: Option<String>,
    }
    let parsed: ApiResponse = resp.json().context("parse Anthropic API response body")?;
    for block in parsed.content {
        if block.ty == "text" {
            if let Some(t) = block.text {
                return Ok(t);
            }
        }
    }
    bail!("Anthropic API response contained no text block")
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

fn backoff_for(attempt: u32) -> Duration {
    // attempt is 1-based by the time we get here; cap at ~16s.
    let secs = 1u64 << attempt.saturating_sub(1).min(4) as u64;
    Duration::from_secs(secs)
}

/// Sleep for `total` while checking `cancellation` every 100ms. Returns
/// `true` if the sleep completed, `false` if cancellation was observed.
fn sleep_with_cancellation(total: Duration, cancellation: &AtomicBool) -> bool {
    let step = Duration::from_millis(100);
    let mut elapsed = Duration::ZERO;
    while elapsed < total {
        if cancellation.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(step);
        elapsed += step;
    }
    true
}

fn truncate_for_error(body: &str) -> String {
    const MAX: usize = 512;
    if body.len() <= MAX {
        return body.to_string();
    }
    let mut idx = MAX;
    while idx > 0 && !body.is_char_boundary(idx) {
        idx -= 1;
    }
    format!("{}... (truncated)", &body[..idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_status_classification() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(200));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(404));
    }

    #[test]
    fn backoff_grows_geometrically() {
        assert_eq!(backoff_for(1), Duration::from_secs(1));
        assert_eq!(backoff_for(2), Duration::from_secs(2));
        assert_eq!(backoff_for(3), Duration::from_secs(4));
        // Capped at 16s.
        assert!(backoff_for(8) <= Duration::from_secs(16));
    }

    #[test]
    fn cancellation_short_circuits_sleep() {
        let cancel = AtomicBool::new(true);
        let start = std::time::Instant::now();
        let completed = sleep_with_cancellation(Duration::from_secs(10), &cancel);
        assert!(!completed);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
