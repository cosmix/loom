//! HMAC-SHA256 and constant-time comparison for operator proofs.
//!
//! Split from `admin_proof` so the proof POLICY (what a capability is bound
//! to, who may mint one, replay consumption) reads separately from the
//! primitive that signs it. The primitive is pinned against the RFC 4231
//! vector in `admin_proof`'s tests; it has no loom-specific behaviour and
//! should need no further change.

use sha2::{Digest, Sha256};

pub(super) const SHA256_LEN: usize = 32;
const SHA256_BLOCK_LEN: usize = 64;

pub(super) fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; SHA256_LEN] {
    let mut key_block = [0u8; SHA256_BLOCK_LEN];
    if key.len() > SHA256_BLOCK_LEN {
        let digest = Sha256::digest(key);
        key_block[..SHA256_LEN].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut inner_pad = [0x36u8; SHA256_BLOCK_LEN];
    let mut outer_pad = [0x5cu8; SHA256_BLOCK_LEN];
    for index in 0..SHA256_BLOCK_LEN {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    outer.finalize().into()
}

pub(super) fn constant_time_eq(expected: &[u8; SHA256_LEN], supplied: &[u8]) -> bool {
    let mut difference = supplied.len() ^ SHA256_LEN;
    for (index, expected_byte) in expected.iter().enumerate() {
        let supplied_byte = supplied.get(index).copied().unwrap_or(0);
        difference |= usize::from(expected_byte ^ supplied_byte);
    }
    difference == 0
}
