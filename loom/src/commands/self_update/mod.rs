//! Binary self-update functionality for the loom CLI.
//!
//! This module backs the `loom update` subcommand. It checks for updates,
//! verifies signatures, and installs new binaries with rollback support.

pub(crate) mod client;
pub(crate) mod install;
pub(crate) mod signature;

#[cfg(test)]
mod tests;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use semver::Version;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use client::{
    create_http_client, download_text_with_limit, download_with_limit, validate_response_status,
};
use install::install_binary;
use signature::{compute_sha256_checksum, verify_binary_signature};

// Repository and version constants
const GITHUB_REPO: &str = "cosmix/loom";
const CURRENT_VERSION: &str = env!("LOOM_VERSION");

/// Single source of truth for which platforms self-update supports and what
/// the release workflow (`.github/workflows/release.yml`) names their
/// binaries: (target triple, published binary asset base name).
const RELEASE_ASSETS: &[(&str, &str)] = &[
    ("x86_64-unknown-linux-gnu", "loom-linux-x86_64"),
    ("x86_64-apple-darwin", "loom-darwin-x86_64"),
    ("aarch64-apple-darwin", "loom-darwin-arm64"),
];

/// Resolve a target triple to its published release binary asset name.
fn release_asset_for_target(target: &str) -> Result<&'static str> {
    RELEASE_ASSETS
        .iter()
        .find(|(triple, _)| *triple == target)
        .map(|(_, name)| *name)
        .ok_or_else(|| anyhow::anyhow!("No release asset for this platform ({target})"))
}

/// Signature asset name for a given published binary asset name.
fn signature_asset_name(binary_name: &str) -> String {
    format!("{binary_name}.minisig")
}

/// GitHub releases API URL for this repository's latest release.
fn releases_api_url() -> String {
    format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest")
}

// Download size limits (exported for tests)
pub(crate) const MAX_BINARY_SIZE: u64 = 50 * 1024 * 1024; // 50MB for binaries
pub(crate) const MAX_SIGNATURE_SIZE: u64 = 4 * 1024; // 4KB for signature files

/// GitHub release information.
#[derive(serde::Deserialize)]
pub(crate) struct Release {
    pub(crate) tag_name: String,
    assets: Vec<Asset>,
}

/// GitHub release asset information.
#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Execute the update command.
pub fn execute() -> Result<()> {
    crate::utils::print_logo_header("Update");
    println!("{}", "Checking for updates...".blue());

    let latest = get_latest_release()?;
    let current = Version::parse(CURRENT_VERSION)?;
    let latest_version = Version::parse(latest.tag_name.trim_start_matches('v'))?;
    let updated = latest_version > current;
    let exe = if updated {
        println!(
            "New version available: {} → {}",
            CURRENT_VERSION.dimmed(),
            latest.tag_name.green().bold()
        );
        update_binary(&latest)?
    } else {
        println!(
            "{} You're running the latest version ({}); refreshing installed assets",
            "✓".green().bold(),
            CURRENT_VERSION
        );
        env::current_exe().context("Failed to get current executable path")?
    };

    if updated {
        verify_installed_version(&exe, &latest.tag_name)
            .map_err(|error| assets_not_refreshed_error(&exe, error))?;
    }
    run_asset_install(&exe).map_err(|error| {
        if updated {
            assets_not_refreshed_error(&exe, error)
        } else {
            error
        }
    })?;

    if updated {
        println!(
            "{} Updated successfully to {}",
            "✓".green().bold(),
            latest.tag_name
        );
    } else {
        println!("{} Assets refreshed", "✓".green().bold());
    }
    Ok(())
}

/// Return an error that describes an asset refresh failure after a binary swap.
fn assets_not_refreshed_error(exe: &Path, error: anyhow::Error) -> anyhow::Error {
    error.context(format!(
        "Binary at {} was updated, but assets were not refreshed; run `loom install-assets`",
        exe.display()
    ))
}

/// Re-executes `exe install-assets` and reports spawn and exit failures separately.
fn run_asset_install(exe: &Path) -> Result<()> {
    let status = Command::new(exe)
        .arg("install-assets")
        .status()
        .with_context(|| format!("Failed to start {} install-assets", exe.display()))?;
    if !status.success() {
        bail!(
            "{} install-assets exited unsuccessfully with {status}",
            exe.display()
        );
    }
    Ok(())
}

/// Check that the binary just installed identifies itself as the release version.
fn verify_installed_version(exe: &Path, release_version: &str) -> Result<()> {
    let output = Command::new(exe)
        .arg("--version")
        .output()
        .with_context(|| format!("Failed to run {} --version", exe.display()))?;
    if !output.status.success() {
        bail!(
            "{} --version exited unsuccessfully with {}",
            exe.display(),
            output.status
        );
    }
    let expected = release_version.trim_start_matches('v');
    let version = String::from_utf8_lossy(&output.stdout);
    if !version.contains(expected) {
        bail!(
            "{} --version did not report the installed release version {expected}",
            exe.display()
        );
    }
    Ok(())
}

/// Fetch the latest release information from GitHub.
pub(crate) fn get_latest_release() -> Result<Release> {
    let url = releases_api_url();
    let client = create_http_client()?;
    let response = client
        .get(&url)
        .send()
        .context("Failed to check for updates")?;

    validate_response_status(&response, "Failed to fetch release info")?;

    response.json().context("Failed to parse release info")
}

/// Get the target triple for the current platform.
fn get_target() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    {
        "unknown"
    }
}

/// Download and install a signed binary, returning the path captured before its swap.
fn update_binary(release: &Release) -> Result<PathBuf> {
    let target = get_target();
    let binary_name = release_asset_for_target(target)?;
    let signature_name = signature_asset_name(binary_name);
    let binary_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == binary_name)
        .ok_or_else(|| anyhow::anyhow!("No binary found for {target}"))?;
    let signature_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == signature_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No signature file found for {target}. Release must include {signature_name}"
            )
        })?;

    let client = create_http_client()?;
    println!("  {} Downloading binary...", "→".blue());
    let response = client
        .get(&binary_asset.browser_download_url)
        .send()
        .context("Failed to download binary")?;
    validate_response_status(&response, "Binary download failed")?;
    let binary = download_with_limit(response, MAX_BINARY_SIZE, "Binary download")?;

    println!("  {} Downloading signature...", "→".blue());
    let response = client
        .get(&signature_asset.browser_download_url)
        .send()
        .context("Failed to download signature")?;
    validate_response_status(&response, "Signature download failed")?;
    let signature = download_text_with_limit(response, MAX_SIGNATURE_SIZE, "Signature download")?;
    verify_and_install_binary(&binary, &signature)
}

/// Verify the downloaded binary before writing it, then atomically install it.
fn verify_and_install_binary(binary: &[u8], signature: &str) -> Result<PathBuf> {
    println!("  {} Verifying cryptographic signature...", "→".blue());
    verify_binary_signature(binary, signature)
        .context("SECURITY ERROR: Binary signature verification failed")?;
    println!("  {} Signature verified successfully", "✓".green());
    println!(
        "  {} SHA-256: {}",
        "ℹ".blue(),
        compute_sha256_checksum(binary).dimmed()
    );

    // Capture this before the swap: Linux then points /proc/self/exe at a deleted backup inode.
    let current_exe = env::current_exe().context("Failed to get current executable path")?;
    install_binary(binary, &current_exe)?;
    println!("  {} Binary updated", "✓".green());
    Ok(current_exe)
}
