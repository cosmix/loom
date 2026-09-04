//! Tests for self-update functionality.

use std::fs;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::TempDir;

use crate::commands::self_update::client::create_http_client;
use crate::commands::self_update::signature::{
    compute_sha256_checksum, verify_binary_signature, MINISIGN_PUBLIC_KEY,
};
use crate::commands::self_update::{
    release_asset_for_target, releases_api_url, run_asset_install, signature_asset_name, Asset,
};

#[path = "tests/install_binary.rs"]
mod install_binary;
#[path = "tests/release_payload.rs"]
mod release_payload;
#[path = "tests/verify_version.rs"]
mod verify_version;

#[test]
fn test_rejects_invalid_signature_format() {
    let result = verify_binary_signature(
        b"valid binary content",
        "not a valid minisign signature format",
    );
    let message = result.unwrap_err().to_string();
    assert!(message.starts_with("Invalid signature format"), "{message}");
}

#[test]
fn test_rejects_empty_signature() {
    assert!(verify_binary_signature(b"some binary data", "").is_err());
}

#[test]
fn test_rejects_malformed_public_key() {
    let result = verify_binary_signature(b"binary content", "untrusted signature: RWT1234567890");
    assert!(result.is_err());
}

/// A corrupted `MINISIGN_PUBLIC_KEY` would otherwise pass the whole suite:
/// every other signature test only asserts rejection, so a broken key still
/// fails those "correctly". This is the one positive case.
#[test]
fn embedded_public_key_parses() {
    assert!(minisign_verify::PublicKey::from_base64(MINISIGN_PUBLIC_KEY).is_ok());
}

#[test]
fn test_compute_sha256_checksum_consistency() {
    let checksum = compute_sha256_checksum(b"test data");
    assert_eq!(checksum, compute_sha256_checksum(b"test data"));
    assert_eq!(checksum.len(), 64);
}

#[test]
fn test_compute_sha256_checksum_different_data() {
    assert_ne!(
        compute_sha256_checksum(b"original"),
        compute_sha256_checksum(b"modified")
    );
}

#[test]
fn test_create_http_client_succeeds() {
    assert!(create_http_client().is_ok());
}

/// An `https_only` client must reject a plaintext URL before dialling out -
/// this is what stops a scheme-downgraded redirect (or a caller mistake)
/// from ever opening an unencrypted connection. Port 1 on the loopback
/// address is never listening, so a client that did attempt to connect would
/// fail with a connection error instead of the scheme rejection asserted
/// here.
#[test]
fn http_client_refuses_a_plaintext_url() {
    let client = create_http_client().unwrap();
    let result = client.get("http://127.0.0.1:1/x").send();
    assert!(
        result.is_err(),
        "expected the https-only client to refuse a plaintext URL"
    );
}

/// Fixture inventory for the binary and signature assets the updater consumes.
fn published_binary_assets() -> Vec<Asset> {
    [
        "loom-linux-x86_64",
        "loom-linux-x86_64.minisig",
        "loom-darwin-arm64",
        "loom-darwin-arm64.minisig",
    ]
    .into_iter()
    .map(|name| Asset {
        name: name.to_string(),
        browser_download_url: format!("https://example.com/{name}"),
    })
    .collect()
}

#[test]
fn test_release_asset_selection_finds_binary_and_signature_for_supported_targets() {
    let assets = published_binary_assets();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        let binary_name = release_asset_for_target(target).unwrap();
        assert!(assets.iter().any(|asset| asset.name == binary_name));
        let signature_name = signature_asset_name(binary_name);
        assert!(assets.iter().any(|asset| asset.name == signature_name));
    }
}

#[test]
fn test_release_asset_selection_rejects_unsupported_linux_arm64() {
    assert!(release_asset_for_target("aarch64-unknown-linux-gnu").is_err());
}

#[test]
fn test_release_asset_selection_rejects_unsupported_macos_intel() {
    assert!(release_asset_for_target("x86_64-apple-darwin").is_err());
}

#[test]
fn test_release_asset_selection_rejects_unknown_target() {
    assert!(release_asset_for_target("unknown").is_err());
}

#[test]
fn test_release_asset_selection_releases_api_url_names_repo() {
    assert_eq!(
        releases_api_url(),
        "https://api.github.com/repos/cosmix/loom/releases/latest"
    );
}

#[cfg(unix)]
fn write_stub(temp_dir: &TempDir, body: &str) -> PathBuf {
    let path = temp_dir.path().join("loom-stub");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

/// Execs of a just-written script transiently fail with `ETXTBSY` when a sibling
/// test thread forks between the write and the spawn: the child holds a duplicate
/// of the write fd until its own exec closes it. Retry until that window passes.
///
/// Generic over the fallible operation so every test in this module that
/// execs a just-written `write_stub` script - not just `run_asset_install` -
/// can reuse the same retry.
#[cfg(unix)]
fn retry_past_etxtbsy<T>(mut attempt: impl FnMut() -> anyhow::Result<T>) -> anyhow::Result<T> {
    const ETXTBSY: i32 = 26; // "Text file busy"
    const MAX_ATTEMPTS: u32 = 50;

    let mut attempts = 0;
    loop {
        attempts += 1;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let is_etxtbsy = error
                    .chain()
                    .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
                    .any(|io_error| io_error.raw_os_error() == Some(ETXTBSY));
                if !is_etxtbsy || attempts >= MAX_ATTEMPTS {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn run_asset_install_invokes_install_assets() {
    let temp_dir = TempDir::new().unwrap();
    let stub = write_stub(
        &temp_dir,
        "printf '%s\\n' \"$@\" > \"$(dirname \"$0\")/argv\"",
    );

    retry_past_etxtbsy(|| run_asset_install(&stub)).unwrap();

    assert_eq!(
        fs::read_to_string(temp_dir.path().join("argv")).unwrap(),
        "install-assets\n"
    );
}

#[cfg(unix)]
#[test]
fn run_asset_install_reports_nonzero_exit() {
    let temp_dir = TempDir::new().unwrap();
    let stub = write_stub(&temp_dir, "exit 7");

    let error = retry_past_etxtbsy(|| run_asset_install(&stub))
        .unwrap_err()
        .to_string();

    assert!(error.contains(&stub.display().to_string()), "{error}");
    assert!(error.contains("exited unsuccessfully"), "{error}");
}

#[cfg(unix)]
#[test]
fn run_asset_install_reports_missing_binary() {
    let temp_dir = TempDir::new().unwrap();
    let missing = temp_dir.path().join("missing-loom");

    let error = run_asset_install(&missing).unwrap_err().to_string();

    assert!(error.contains(&missing.display().to_string()), "{error}");
    assert!(error.contains("Failed to start"), "{error}");
}
