//! Cryptographic signature verification for self-update binaries.
//!
//! Uses minisign for verifying release signatures.

use anyhow::Result;
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

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
