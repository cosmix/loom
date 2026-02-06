//! Self-update functionality for the loom CLI.
//!
//! This module handles checking for updates, downloading new versions,
//! verifying signatures, and installing updates with rollback support.

pub(crate) mod client;
pub(crate) mod install;
pub(crate) mod signature;
pub(crate) mod zip;

#[cfg(test)]
mod tests;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use semver::Version;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use client::{
    create_http_client, download_text_with_limit, download_with_limit, validate_response_status,
};
use install::install_binary;
use signature::{
    compute_sha256_checksum, parse_checksums, verify_binary_signature, verify_checksum,
};
use zip::{
    safe_extract_path, validate_zip_entry, LimitedReader, MAX_TOTAL_EXTRACTED_SIZE,
    MAX_UNCOMPRESSED_SIZE,
};

// Import ZipArchive from the zip crate for in-memory extraction
use ::zip::ZipArchive;

// Repository and version constants
const GITHUB_REPO: &str = "cosmix/claude-loom";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Download size limits (exported for tests)
pub(crate) const MAX_BINARY_SIZE: u64 = 50 * 1024 * 1024; // 50MB for binaries
pub(crate) const MAX_TEXT_SIZE: u64 = 10 * 1024 * 1024; // 10MB for text files
pub(crate) const MAX_SIGNATURE_SIZE: u64 = 4 * 1024; // 4KB for signature files

/// GitHub release information.
#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

/// GitHub release asset information.
#[derive(serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// Execute self-update command.
pub fn execute() -> Result<()> {
    println!("{}", "Checking for updates...".blue());

    let latest = get_latest_release()?;
    let current = Version::parse(CURRENT_VERSION)?;
    let latest_version = Version::parse(latest.tag_name.trim_start_matches('v'))?;

    if latest_version <= current {
        println!(
            "{} You're running the latest version ({})",
            "✓".green().bold(),
            CURRENT_VERSION
        );
        return Ok(());
    }

    println!(
        "New version available: {} → {}",
        CURRENT_VERSION.dimmed(),
        latest.tag_name.green().bold()
    );

    // Update binary
    update_binary(&latest)?;

    // Update agents, skills, CLAUDE.md
    update_config_files(&latest)?;

    println!(
        "{} Updated successfully to {}",
        "✓".green().bold(),
        latest.tag_name
    );
    Ok(())
}

/// Fetch the latest release information from GitHub.
fn get_latest_release() -> Result<Release> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
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

/// Download and install the new binary with signature verification.
fn update_binary(release: &Release) -> Result<()> {
    let target = get_target();
    if target == "unknown" {
        bail!("Unsupported platform for self-update");
    }

    let binary_name = format!("loom-{target}");
    let signature_name = format!("{binary_name}.minisig");

    // Find binary asset
    let binary_asset = release
        .assets
        .iter()
        .find(|a| a.name == binary_name)
        .ok_or_else(|| anyhow::anyhow!("No binary found for {target}"))?;

    // Find signature asset - REQUIRED for security
    let signature_asset = release
        .assets
        .iter()
        .find(|a| a.name == signature_name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No signature file found for {target}. Release must include {signature_name}"
            )
        })?;

    let client = create_http_client()?;

    // Download binary
    println!("  {} Downloading binary...", "→".blue());
    let binary_response = client
        .get(&binary_asset.browser_download_url)
        .send()
        .context("Failed to download binary")?;
    validate_response_status(&binary_response, "Binary download failed")?;
    let binary_bytes = download_with_limit(binary_response, MAX_BINARY_SIZE, "Binary download")?;

    // Download signature
    println!("  {} Downloading signature...", "→".blue());
    let sig_response = client
        .get(&signature_asset.browser_download_url)
        .send()
        .context("Failed to download signature")?;
    validate_response_status(&sig_response, "Signature download failed")?;
    let signature_content =
        download_text_with_limit(sig_response, MAX_SIGNATURE_SIZE, "Signature download")?;

    // CRITICAL: Verify signature BEFORE writing binary to disk
    println!("  {} Verifying cryptographic signature...", "→".blue());
    verify_binary_signature(&binary_bytes, &signature_content)
        .context("SECURITY ERROR: Binary signature verification failed")?;
    println!("  {} Signature verified successfully", "✓".green());

    // Compute and log checksum for defense-in-depth auditing
    let checksum = compute_sha256_checksum(&binary_bytes);
    println!("  {} SHA-256: {}", "ℹ".blue(), checksum.dimmed());

    // Get current executable path
    let current_exe = env::current_exe().context("Failed to get current executable path")?;

    // Install the new binary with rollback mechanism
    install_binary(&binary_bytes, &current_exe)?;

    println!("  {} Binary updated", "✓".green());
    Ok(())
}

/// Get the Claude configuration directory path.
fn get_claude_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
    Ok(home.join(".claude"))
}

/// Update configuration files (CLAUDE.md, agents, skills).
fn update_config_files(release: &Release) -> Result<()> {
    let claude_dir = get_claude_dir()?;
    let client = create_http_client()?;

    // Try to download checksums file for verification
    let checksums =
        if let Some(checksum_asset) = release.assets.iter().find(|a| a.name == "checksums.txt") {
            println!("  {} Downloading checksums...", "→".blue());
            let response = client
                .get(&checksum_asset.browser_download_url)
                .send()
                .context("Failed to download checksums")?;
            validate_response_status(&response, "Checksums download failed")?;
            let content = download_text_with_limit(response, MAX_TEXT_SIZE, "Checksums download")?;
            let parsed = parse_checksums(&content);
            println!(
                "  {} Checksums loaded ({} entries)",
                "✓".green(),
                parsed.len()
            );
            Some(parsed)
        } else {
            eprintln!(
                "  {} No checksums.txt in release, skipping verification",
                "!".yellow().bold()
            );
            None
        };

    // Update CLAUDE.md.template -> CLAUDE.md
    if let Some(asset) = release
        .assets
        .iter()
        .find(|a| a.name == "CLAUDE.md.template")
    {
        println!("  {} Downloading CLAUDE.md.template...", "→".blue());
        let response = client
            .get(&asset.browser_download_url)
            .send()
            .context("Failed to download CLAUDE.md.template")?;
        validate_response_status(&response, "CLAUDE.md.template download failed")?;
        let content =
            download_text_with_limit(response, MAX_TEXT_SIZE, "CLAUDE.md.template download")?;

        // Verify checksum if available
        if let Some(ref checksums) = checksums {
            if let Some(expected) = checksums.get("CLAUDE.md.template") {
                verify_checksum(content.as_bytes(), expected, "CLAUDE.md.template")?;
                println!("  {} CLAUDE.md.template checksum verified", "✓".green());
            }
        }

        // Save with timestamp header
        save_with_header(&content, &claude_dir.join("CLAUDE.md"))?;
        println!("  {} CLAUDE.md updated", "✓".green());
    }

    // Update agents
    if let Some(asset) = release.assets.iter().find(|a| a.name == "agents.zip") {
        println!("  {} Downloading agents...", "→".blue());
        let agents_dir = claude_dir.join("agents");
        download_verify_and_extract_zip(
            &client,
            &asset.browser_download_url,
            &agents_dir,
            "agents.zip",
            &checksums,
        )?;
        println!("  {} agents/ updated", "✓".green());
    }

    // Update skills
    if let Some(asset) = release.assets.iter().find(|a| a.name == "skills.zip") {
        println!("  {} Downloading skills...", "→".blue());
        let skills_dir = claude_dir.join("skills");
        download_verify_and_extract_zip(
            &client,
            &asset.browser_download_url,
            &skills_dir,
            "skills.zip",
            &checksums,
        )?;
        println!("  {} skills/ updated", "✓".green());
    }

    Ok(())
}

/// Save text content with a timestamp header.
fn save_with_header(content: &str, dest: &Path) -> Result<()> {
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");
    let full_content = format!(
        "# ───────────────────────────────────────────────────────────\n\
         # claude-loom | updated {timestamp}\n\
         # ───────────────────────────────────────────────────────────\n\n\
         {content}"
    );

    fs::write(dest, full_content).context("Failed to write file")?;
    Ok(())
}

/// Download, verify checksum, and extract a zip file.
fn download_verify_and_extract_zip(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    asset_name: &str,
    checksums: &Option<HashMap<String, String>>,
) -> Result<()> {
    let response = client.get(url).send().context("Failed to download zip")?;
    validate_response_status(&response, "Zip download failed")?;
    let bytes = download_with_limit(response, zip::MAX_ZIP_SIZE, asset_name)?;

    // Verify checksum if available
    if let Some(ref checksums) = checksums {
        if let Some(expected) = checksums.get(asset_name) {
            verify_checksum(&bytes, expected, asset_name)?;
            println!("  {} {} checksum verified", "✓".green(), asset_name);
        }
    }

    // Extract using zip crate directly (replicating safe extraction from zip.rs)
    let cursor = Cursor::new(&bytes);
    let mut archive = ZipArchive::new(cursor).context("Failed to open zip archive")?;

    // Pre-validate all entries before extraction (fail fast on malicious archives)
    let mut total_uncompressed_size: u64 = 0;
    for i in 0..archive.len() {
        let file = archive.by_index(i).context("Failed to read zip entry")?;

        // Validate against zip bombs
        validate_zip_entry(&file)?;

        // Track total size with overflow protection
        total_uncompressed_size = total_uncompressed_size
            .checked_add(file.size())
            .ok_or_else(|| {
                anyhow::anyhow!("Total uncompressed size overflow - possible zip bomb")
            })?;

        if total_uncompressed_size > MAX_TOTAL_EXTRACTED_SIZE {
            bail!(
                "Total uncompressed size {} exceeds maximum {} bytes - possible zip bomb",
                total_uncompressed_size,
                MAX_TOTAL_EXTRACTED_SIZE
            );
        }

        // Validate path safety
        let entry_name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file.mangled_name().to_string_lossy().to_string());

        if !entry_name.is_empty() {
            safe_extract_path(dest, &entry_name)?;
        }
    }

    // Backup existing directory (only after validation passes)
    if dest.exists() {
        let backup = dest.with_extension("bak");
        if backup.exists() {
            fs::remove_dir_all(&backup).ok();
        }
        fs::rename(dest, &backup).context("Failed to backup directory")?;
    }

    // Re-open archive for extraction (we consumed it during validation)
    let cursor = Cursor::new(&bytes);
    let mut archive = ZipArchive::new(cursor).context("Failed to reopen zip archive")?;

    fs::create_dir_all(dest)?;

    // Extract with validated paths
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;

        // Get safe entry name
        let entry_name = file
            .enclosed_name()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| file.mangled_name().to_string_lossy().to_string());

        if entry_name.is_empty() {
            continue;
        }

        let outpath = safe_extract_path(dest, &entry_name)?;

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)?;
            }

            // Extract with size limit enforcement during decompression
            let mut outfile = fs::File::create(&outpath)
                .with_context(|| format!("Failed to create file: {}", outpath.display()))?;

            let mut limited_reader = LimitedReader::new(&mut file, MAX_UNCOMPRESSED_SIZE);
            std::io::copy(&mut limited_reader, &mut outfile)
                .with_context(|| format!("Failed to extract file: {}", entry_name))?;
        }
    }

    // Cleanup backup
    let backup = dest.with_extension("bak");
    if backup.exists() {
        fs::remove_dir_all(&backup).ok();
    }

    Ok(())
}
