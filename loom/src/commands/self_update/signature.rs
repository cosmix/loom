//! Cryptographic signature verification for self-update binaries.
//!
//! Uses minisign for release signatures and SHA-256 for audit logging.

use anyhow::Result;
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

/// Trust anchor shared with the standalone bootstrap installer.
pub(crate) fn minisign_public_key() -> &'static str {
    super::release_contract::value("MINISIGN_PUBLIC_KEY")
}

/// Verify the cryptographic signature of downloaded binary content.
/// Uses minisign signature format for verification.
/// Returns Ok(()) if signature is valid, Err with detailed message otherwise.
pub(crate) fn verify_binary_signature(
    binary_content: &[u8],
    signature_content: &str,
) -> Result<()> {
    let public_key = PublicKey::from_base64(minisign_public_key())
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

/// Compute SHA-256 checksum of binary content for audit logging.
/// Returns the hex-encoded hash string.
pub(crate) fn compute_sha256_checksum(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    hex::encode(result)
}
