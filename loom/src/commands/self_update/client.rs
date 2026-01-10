//! HTTP client functionality for self-update operations.
//!
//! Provides secure HTTP client creation with timeouts and size-limited downloads.

use anyhow::{bail, Context, Result};
use reqwest::blocking::{Client, Response};
use std::io::Read;
use std::time::Duration;

// HTTP Security Constants
pub(crate) const HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
pub(crate) const HTTP_REQUEST_TIMEOUT_SECS: u64 = 120; // Total request timeout (includes connection + transfer)

/// Create an HTTP client with security-focused timeout configuration.
/// Prevents indefinite hangs on slow or unresponsive servers.
/// - connect_timeout: Maximum time to establish a TCP connection
/// - timeout: Maximum time for the entire request (connection + data transfer)
pub(crate) fn create_http_client() -> Result<Client> {
    Client::builder()
        .connect_timeout(Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS))
        .user_agent("loom-self-update")
        .build()
        .context("Failed to create HTTP client")
}

/// Validate HTTP response status code and return a descriptive error if not successful.
pub(crate) fn validate_response_status(response: &Response, context: &str) -> Result<()> {
    if !response.status().is_success() {
        let status = response.status();
        bail!(
            "{}: HTTP {} - {}",
            context,
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown error")
        );
    }
    Ok(())
}

/// Download content with size limit enforcement.
/// Checks Content-Length header first, then enforces limit during streaming.
pub(crate) fn download_with_limit(
    response: Response,
    max_size: u64,
    context: &str,
) -> Result<Vec<u8>> {
    // Check Content-Length header if available
    if let Some(content_length) = response.content_length() {
        if content_length > max_size {
            bail!(
                "{context}: Content-Length {content_length} bytes exceeds maximum allowed size of {max_size} bytes"
            );
        }
    }

    // Stream the response with size limit enforcement
    let mut bytes = Vec::new();
    let mut reader = response;
    let mut total_read: u64 = 0;
    let mut buffer = [0u8; 8192];

    loop {
        let n = reader
            .read(&mut buffer)
            .context("Failed to read response body")?;
        if n == 0 {
            break;
        }
        total_read += n as u64;
        if total_read > max_size {
            bail!("{context}: Download size exceeds maximum allowed size of {max_size} bytes");
        }
        bytes.extend_from_slice(&buffer[..n]);
    }

    Ok(bytes)
}

/// Download text content with size limit enforcement.
pub(crate) fn download_text_with_limit(
    response: Response,
    max_size: u64,
    context: &str,
) -> Result<String> {
    let bytes = download_with_limit(response, max_size, context)?;
    String::from_utf8(bytes).context("Response contains invalid UTF-8")
}
