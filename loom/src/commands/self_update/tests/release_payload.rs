//! Tests for the GitHub release payload shape and the missing-signature-asset
//! failure path in `update_binary`.

use crate::commands::self_update::{update_binary, Release};

use super::published_binary_assets;

#[test]
fn release_payload_deserializes_the_tag_and_assets() {
    let json = r#"{
        "tag_name": "v1.2.3",
        "assets": [
            {
                "name": "loom-linux-x86_64",
                "browser_download_url": "https://example.com/loom-linux-x86_64"
            }
        ]
    }"#;

    let release: Release = serde_json::from_str(json).unwrap();

    assert_eq!(release.tag_name, "v1.2.3");
    assert_eq!(release.assets.len(), 1);
    assert_eq!(release.assets[0].name, "loom-linux-x86_64");
    assert_eq!(
        release.assets[0].browser_download_url,
        "https://example.com/loom-linux-x86_64"
    );
}

/// `update_binary` resolves the signature asset before any network access,
/// so a release missing the platform's `.minisig` file must be reported
/// without downloading anything. This assumes the test host is one of the
/// supported release targets (`RELEASE_ASSETS` in `mod.rs`), matching every
/// other test in this module that resolves the host's own target triple.
#[test]
fn update_binary_reports_a_missing_signature_asset() {
    let assets: Vec<_> = published_binary_assets()
        .into_iter()
        .filter(|asset| !asset.name.ends_with(".minisig"))
        .collect();
    let release = Release {
        tag_name: "v1.2.3".to_string(),
        assets,
    };

    let error = update_binary(&release).unwrap_err().to_string();

    assert!(error.contains("No signature file found"), "{error}");
}
