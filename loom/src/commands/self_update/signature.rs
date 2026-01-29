//! Cryptographic signature verification for self-update binaries.
//!
//! Uses minisign for verifying release signatures and SHA256 checksums for config files.

use anyhow::{bail, Result};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Minisign public key for verifying release signatures.
/// Key ID: 8E9D22D875357EC7
/// Generated: 2026-01-06
pub(crate) const MINISIGN_PUBLIC_KEY: &str =
    "RWTHfjV12CKdjuXF6DPYXsOoneV6zG4nt4Qd1DFe7JzSIXTXKfRJPHjJ";

/// Verify the cryptographic signature of downloaded binary content.
/// Uses minisign signature format for verification.
/// Returns Ok(()) if signature is valid, Err with detailed message otherwise.
pub(crate) fn verify_binary_signature(
    binary_content: &[u8],
    signature_content: &str,
) -> Result<()> {
    let public_key = PublicKey::from_base64(MINISIGN_PUBLIC_KEY)
        .map_err(|e| anyhow::anyhow!("Invalid embedded public key: {e}"))?;

    let signature = Signature::decode(signature_content)
        .map_err(|e| anyhow::anyhow!("Invalid signature format: {e}"))?;

    public_key
        .verify(binary_content, &signature, false)
        .map_err(|e| {
            anyhow::anyhow!(
                "Binary signature verification FAILED - possible tampering detected: {e}"
            )
        })?;

    Ok(())
}

/// Compute SHA-256 checksum of binary content for logging and verification.
/// Returns the hex-encoded hash string.
pub(crate) fn compute_sha256_checksum(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Parse a SHA256 checksums file (standard sha256sum format).
/// Format: `<64-char-hex>  <filename>` (note: two spaces)
///
/// Example checksums file:
/// ```text
/// e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  agents.zip
/// d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f  skills.zip
/// ```
#[allow(dead_code)]
pub(crate) fn parse_checksums(content: &str) -> HashMap<String, String> {
    let mut checksums = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue; // Skip empty lines and comments
        }
        let parts: Vec<&str> = line.splitn(2, "  ").collect(); // note: two spaces
        if parts.len() == 2 && parts[0].len() == 64 {
            // Validate hex string
            if parts[0].chars().all(|c| c.is_ascii_hexdigit()) {
                checksums.insert(parts[1].to_string(), parts[0].to_string());
            }
        }
    }
    checksums
}

/// Verify content matches expected SHA256 checksum.
/// Returns Ok(()) if checksum matches, Err with details if verification fails.
#[allow(dead_code)]
pub(crate) fn verify_checksum(content: &[u8], expected: &str, asset_name: &str) -> Result<()> {
    let actual = compute_sha256_checksum(content);
    if actual != expected {
        bail!(
            "Checksum verification failed for {}: expected {}, got {}",
            asset_name,
            expected,
            actual
        );
    }
    Ok(())
}
