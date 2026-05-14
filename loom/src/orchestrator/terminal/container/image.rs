//! Container image cache + build orchestration.
//!
//! Renders the embedded Dockerfile template against language flags
//! derived from the project fingerprint, materialises it under the
//! per-user image-cache root, invokes the detected runtime's `build`
//! command, and records the resulting image digest for future cache
//! hits.
//!
//! Cache layout (one subdirectory per fingerprint):
//!
//! ```text
//! <cache_dir>/
//!   <fingerprint>/
//!     Dockerfile        (rendered)
//!     firewall.sh       (copied from embedded resource)
//!     entrypoint.sh     (copied from embedded resource)
//!     image.digest      (sha256:... or repo@sha256:...)
//!     built_at          (RFC3339 timestamp)
//! ```
//!
//! Cache-root resolution (in priority order):
//!
//!   1. `$LOOM_CACHE_DIR` — used as-is (must already be a directory
//!      reserved for loom image cache).
//!   2. Linux: `${XDG_CACHE_HOME:-$HOME/.cache}/loom/images`.
//!   3. macOS: `$HOME/Library/Caches/loom/images`.

use anyhow::{anyhow, bail, Context, Result};
use handlebars::Handlebars;
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

use super::resources::{DOCKERFILE_TEMPLATE, ENTRYPOINT_SCRIPT, FIREWALL_SCRIPT};
use super::runtime::Runtime;

/// Upstream base image. The Dockerfile pins by digest via the
/// `BASE_IMAGE_DIGEST` build-arg; we resolve that digest at build time
/// by pulling this ref and reading its RepoDigest.
const BASE_IMAGE_REF: &str = "mcr.microsoft.com/devcontainers/base:ubuntu-24.04";

/// Resolve the image cache root directory.
///
/// `$LOOM_CACHE_DIR` (when set & non-empty) wins outright — operators
/// who supply it are pointing at the cache root, not a parent. Falls
/// back to a platform-appropriate default rooted at the user's cache
/// directory.
pub fn cache_dir() -> Result<PathBuf> {
    if let Some(val) = std::env::var_os("LOOM_CACHE_DIR") {
        if !val.is_empty() {
            return Ok(PathBuf::from(val));
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .ok_or_else(|| anyhow!("Cannot resolve image cache dir: $HOME is not set"))?;
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Caches")
            .join("loom")
            .join("images"));
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
            if !xdg.is_empty() {
                return Ok(PathBuf::from(xdg).join("loom").join("images"));
            }
        }
        let home = std::env::var_os("HOME")
            .ok_or_else(|| anyhow!("Cannot resolve image cache dir: $HOME is not set"))?;
        Ok(PathBuf::from(home)
            .join(".cache")
            .join("loom")
            .join("images"))
    }
}

/// Derive the language flags from a fingerprint string.
///
/// Fingerprints have the shape `<langs>-<hash>` where `<langs>` is a
/// hyphen-joined sorted set of detected languages (e.g.
/// `"rust-typescript"`) and `<hash>` is an 8+ character content hash.
/// We split on the **last** hyphen so multi-language prefixes stay
/// intact.
///
/// Special case: `"base-<hash>"` denotes the language-less base image
/// — no toolchain flags are emitted.
fn language_flags(fingerprint: &str) -> (bool, bool, bool, bool) {
    let prefix = fingerprint
        .rsplit_once('-')
        .map(|(p, _)| p)
        .unwrap_or(fingerprint);

    let mut has_rust = false;
    let mut has_typescript = false;
    let mut has_python = false;
    let mut has_go = false;

    for token in prefix.split('-') {
        match token {
            "rust" => has_rust = true,
            "typescript" => has_typescript = true,
            "python" => has_python = true,
            "go" => has_go = true,
            _ => {}
        }
    }

    (has_rust, has_typescript, has_python, has_go)
}

/// Render the embedded Dockerfile template for the given fingerprint.
///
/// Pure helper (no I/O); used by `ensure_image` and exercised
/// directly by tests.
pub fn render_dockerfile(fingerprint: &str) -> Result<String> {
    let (has_rust, has_typescript, has_python, has_go) = language_flags(fingerprint);
    let mut hb = Handlebars::new();
    hb.set_strict_mode(false);
    hb.register_template_string("Dockerfile", DOCKERFILE_TEMPLATE)
        .context("Failed to register embedded Dockerfile template")?;
    let ctx = json!({
        "has_rust": has_rust,
        "has_typescript": has_typescript,
        "has_python": has_python,
        "has_go": has_go,
    });
    hb.render("Dockerfile", &ctx)
        .context("Failed to render Dockerfile template")
}

/// Ensure an image for the given fingerprint exists; build it if not.
///
/// Returns the image digest (suitable for use as `image_ref` in
/// `ContainerBackend`). When `force` is true, skips the cache hit
/// check and always rebuilds.
pub fn ensure_image(fingerprint: &str, runtime: Runtime, force: bool) -> Result<String> {
    let dir = cache_dir()?.join(fingerprint);
    let digest_file = dir.join("image.digest");

    // Cache-hit path: digest file present AND runtime can still see the
    // image. If the user nukes their image store but the digest file
    // remains, we fall through and rebuild.
    if !force && digest_file.exists() {
        let digest = std::fs::read_to_string(&digest_file)
            .with_context(|| format!("Failed to read {}", digest_file.display()))?
            .trim()
            .to_string();
        if !digest.is_empty() && runtime_has_image(runtime, &digest) {
            return Ok(digest);
        }
    }

    // Rebuild path.
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create image cache dir {}", dir.display()))?;

    let rendered = render_dockerfile(fingerprint)?;
    let dockerfile_path = dir.join("Dockerfile");
    std::fs::write(&dockerfile_path, rendered)
        .with_context(|| format!("Failed to write {}", dockerfile_path.display()))?;
    std::fs::write(dir.join("firewall.sh"), FIREWALL_SCRIPT)
        .with_context(|| format!("Failed to write firewall.sh under {}", dir.display()))?;
    std::fs::write(dir.join("entrypoint.sh"), ENTRYPOINT_SCRIPT)
        .with_context(|| format!("Failed to write entrypoint.sh under {}", dir.display()))?;

    let base_digest = resolve_base_image_digest(runtime)?;

    let tag = format!("loom/base:{fingerprint}");
    let status = Command::new(runtime.binary())
        .arg("build")
        .arg("-t")
        .arg(&tag)
        .arg("--build-arg")
        .arg(format!("BASE_IMAGE_DIGEST={base_digest}"))
        .arg("--progress=plain")
        .arg(".")
        .current_dir(&dir)
        .status()
        .with_context(|| {
            format!(
                "Failed to invoke `{} build` for fingerprint {fingerprint}",
                runtime.binary()
            )
        })?;
    if !status.success() {
        bail!(
            "`{} build -t {tag}` failed (exit {:?}) for fingerprint {fingerprint}",
            runtime.binary(),
            status.code()
        );
    }

    let digest = resolve_image_digest(runtime, &tag)?;
    std::fs::write(&digest_file, &digest)
        .with_context(|| format!("Failed to write {}", digest_file.display()))?;
    std::fs::write(dir.join("built_at"), chrono::Utc::now().to_rfc3339())
        .with_context(|| format!("Failed to write built_at under {}", dir.display()))?;
    std::fs::write(dir.join("base_digest"), &base_digest)
        .with_context(|| format!("Failed to write base_digest under {}", dir.display()))?;

    Ok(digest)
}

/// Pull the upstream base image and resolve its repo digest.
///
/// Returns a `sha256:...` string suitable for substitution into the
/// Dockerfile's `BASE_IMAGE_DIGEST` build-arg. Pulling first ensures
/// the local image store has a RepoDigest populated; `inspect` then
/// returns the platform-specific digest the runtime resolved during
/// pull. Multi-arch manifests are handled by the runtime — we read
/// whatever it picked for the current platform.
fn resolve_base_image_digest(runtime: Runtime) -> Result<String> {
    let pull = Command::new(runtime.binary())
        .args(["pull", BASE_IMAGE_REF])
        .status()
        .with_context(|| {
            format!(
                "Failed to invoke `{} pull {BASE_IMAGE_REF}`",
                runtime.binary()
            )
        })?;
    if !pull.success() {
        bail!(
            "`{} pull {BASE_IMAGE_REF}` failed (exit {:?})",
            runtime.binary(),
            pull.code()
        );
    }

    let out = Command::new(runtime.binary())
        .args([
            "inspect",
            "--format",
            "{{index .RepoDigests 0}}",
            BASE_IMAGE_REF,
        ])
        .output()
        .with_context(|| {
            format!(
                "Failed to invoke `{} inspect {BASE_IMAGE_REF}`",
                runtime.binary()
            )
        })?;
    if !out.status.success() {
        bail!(
            "`{} inspect {BASE_IMAGE_REF}` failed: {}",
            runtime.binary(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let repo_digest = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if repo_digest.is_empty() || repo_digest == "<no value>" || repo_digest == "[]" {
        bail!(
            "`{} inspect {BASE_IMAGE_REF}` returned no RepoDigest — the runtime may not have pulled the image from a registry",
            runtime.binary()
        );
    }
    // RepoDigests format: "mcr.microsoft.com/devcontainers/base@sha256:...".
    // Strip the repo prefix to leave just "sha256:...".
    let digest = repo_digest
        .rsplit_once('@')
        .map(|(_, d)| d.to_string())
        .unwrap_or(repo_digest.clone());
    if !digest.starts_with("sha256:") {
        bail!("Resolved digest '{digest}' for {BASE_IMAGE_REF} is not a sha256 reference");
    }
    Ok(digest)
}

/// Check whether the runtime's image store can still see `digest`.
/// Returns false on any inspect failure — the caller treats that as a
/// cache miss and rebuilds.
fn runtime_has_image(runtime: Runtime, digest: &str) -> bool {
    Command::new(runtime.binary())
        .args(["image", "inspect", digest])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the image digest for a freshly-built tag.
///
/// Prefers `RepoDigests[0]` (push-resolved sha256) but falls back to
/// `.Id` (local-only `sha256:...`) for the common case where the
/// image has never been pushed to a registry.
fn resolve_image_digest(runtime: Runtime, tag: &str) -> Result<String> {
    let try_format = |fmt: &str| -> Result<Option<String>> {
        let out = Command::new(runtime.binary())
            .args(["inspect", "--format", fmt, tag])
            .output()
            .with_context(|| format!("Failed to invoke `{} inspect {tag}`", runtime.binary()))?;
        if !out.status.success() {
            return Ok(None);
        }
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() || s == "<no value>" || s == "[]" {
            Ok(None)
        } else {
            Ok(Some(s))
        }
    };

    if let Some(repo_digest) = try_format("{{index .RepoDigests 0}}")? {
        return Ok(repo_digest);
    }
    if let Some(id) = try_format("{{.Id}}")? {
        return Ok(id);
    }
    Err(anyhow!(
        "Could not resolve image digest for {tag} via `{} inspect`",
        runtime.binary()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    // --- language_flags ---

    #[test]
    fn language_flags_strips_hash() {
        assert_eq!(language_flags("rust-12345678"), (true, false, false, false));
    }

    #[test]
    fn language_flags_multilang() {
        assert_eq!(
            language_flags("rust-typescript-12345678"),
            (true, true, false, false)
        );
    }

    #[test]
    fn language_flags_base_only() {
        assert_eq!(
            language_flags("base-12345678"),
            (false, false, false, false)
        );
    }

    #[test]
    fn language_flags_all() {
        assert_eq!(
            language_flags("go-python-rust-typescript-abcd1234"),
            (true, true, true, true)
        );
    }

    // --- render_dockerfile ---
    //
    // These tests use unique INSTALL-COMMAND markers (RUSTUP_INIT_SHA256,
    // npm install -g typescript, UV uv-installer, GO_ARCHIVE_SHA256) rather
    // than language NAMES — the header pin-rotation comment legitimately
    // mentions "rustup", "go", etc. as documentation, so a contains("rustup")
    // check would pass even when has_rust is false and the install block is
    // not emitted. Markers below appear ONLY inside their respective
    // `{{#if has_LANG}}` blocks.

    const RUST_MARKER: &str = "RUSTUP_INIT_SHA256";
    const TS_MARKER: &str = "npm install -g typescript";
    const PYTHON_MARKER: &str = "# Python toolchain";
    const GO_MARKER: &str = "GO_ARCHIVE_SHA256";

    #[test]
    fn render_dockerfile_emits_rust_when_fingerprint_has_rust() {
        let out = render_dockerfile("rust-12345678").unwrap();
        assert!(out.contains(RUST_MARKER), "expected rust install in: {out}");
        assert!(
            !out.contains(TS_MARKER),
            "should not contain typescript install: {out}"
        );
    }

    #[test]
    fn render_dockerfile_emits_multiple_languages() {
        let out = render_dockerfile("rust-typescript-12345678").unwrap();
        assert!(out.contains(RUST_MARKER));
        assert!(out.contains(TS_MARKER));
    }

    #[test]
    fn render_dockerfile_base_only() {
        let out = render_dockerfile("base-12345678").unwrap();
        assert!(!out.contains(RUST_MARKER));
        assert!(!out.contains(GO_MARKER));
        assert!(!out.contains(PYTHON_MARKER));
        assert!(!out.contains(TS_MARKER));
    }

    #[test]
    fn render_dockerfile_always_includes_base_image() {
        let out = render_dockerfile("base-12345678").unwrap();
        assert!(out.contains("FROM mcr.microsoft.com/devcontainers/base"));
        assert!(out.contains("loom-entrypoint.sh"));
        assert!(out.contains("loom-firewall.sh"));
        assert!(out.contains("gosu"));
    }

    #[test]
    fn render_dockerfile_python_and_go() {
        let out = render_dockerfile("go-python-deadbeef").unwrap();
        assert!(out.contains(PYTHON_MARKER));
        assert!(out.contains(GO_MARKER));
        assert!(!out.contains(RUST_MARKER));
        assert!(!out.contains(TS_MARKER));
    }

    // Regression: rustup-init runs as root during image build; if we let it
    // default to $HOME, cargo lands in /root/.cargo and the unprivileged
    // `loom` runtime user can't traverse /root (mode 0700) to reach the
    // symlinks under /usr/local/bin. CARGO_HOME/RUSTUP_HOME must be pinned
    // to a world-readable location so cargo is usable from the runtime user.
    #[test]
    fn render_dockerfile_rust_install_is_world_readable() {
        let out = render_dockerfile("rust-deadbeef").unwrap();
        assert!(
            out.contains("CARGO_HOME=/usr/local/cargo"),
            "CARGO_HOME must be pinned to /usr/local/cargo: {out}"
        );
        assert!(
            out.contains("RUSTUP_HOME=/usr/local/rustup"),
            "RUSTUP_HOME must be pinned to /usr/local/rustup: {out}"
        );
        // Catch the specific broken pattern: symlinking from /root/.cargo
        // into /usr/local/bin. Comments may mention /root/.cargo as
        // documentation, so don't reject the substring outright.
        assert!(
            !out.contains("ln -sf /root/.cargo"),
            "rust install must not symlink out of /root/.cargo: {out}"
        );
    }

    // Regression: same class of bug for uv. The default installer drops
    // into /root/.local/bin, which is unreachable from the runtime user.
    #[test]
    fn render_dockerfile_python_install_is_world_readable() {
        let out = render_dockerfile("python-deadbeef").unwrap();
        assert!(
            out.contains("UV_INSTALL_DIR=/usr/local/bin"),
            "uv must install directly into /usr/local/bin: {out}"
        );
        assert!(
            !out.contains("ln -sf /root/.local"),
            "uv install must not symlink out of /root/.local: {out}"
        );
    }

    // --- cache_dir ---

    #[test]
    #[serial]
    fn cache_dir_honors_loom_cache_dir_env() {
        let temp = TempDir::new().unwrap();
        let prev = std::env::var_os("LOOM_CACHE_DIR");
        // SAFETY: serialized via #[serial]; no other thread mutates env.
        unsafe {
            std::env::set_var("LOOM_CACHE_DIR", temp.path());
        }
        let dir = cache_dir().unwrap();
        assert_eq!(dir, temp.path());
        unsafe {
            match prev {
                Some(v) => std::env::set_var("LOOM_CACHE_DIR", v),
                None => std::env::remove_var("LOOM_CACHE_DIR"),
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    fn cache_dir_linux_default() {
        let temp = TempDir::new().unwrap();
        let prev_home = std::env::var_os("HOME");
        let prev_xdg = std::env::var_os("XDG_CACHE_HOME");
        let prev_lcd = std::env::var_os("LOOM_CACHE_DIR");
        unsafe {
            std::env::remove_var("LOOM_CACHE_DIR");
            std::env::remove_var("XDG_CACHE_HOME");
            std::env::set_var("HOME", temp.path());
        }
        let dir = cache_dir().unwrap();
        assert_eq!(dir, temp.path().join(".cache").join("loom").join("images"));
        unsafe {
            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            if let Some(v) = prev_xdg {
                std::env::set_var("XDG_CACHE_HOME", v);
            }
            if let Some(v) = prev_lcd {
                std::env::set_var("LOOM_CACHE_DIR", v);
            }
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[serial]
    fn cache_dir_linux_xdg_override() {
        let temp = TempDir::new().unwrap();
        let prev_xdg = std::env::var_os("XDG_CACHE_HOME");
        let prev_lcd = std::env::var_os("LOOM_CACHE_DIR");
        unsafe {
            std::env::remove_var("LOOM_CACHE_DIR");
            std::env::set_var("XDG_CACHE_HOME", temp.path());
        }
        let dir = cache_dir().unwrap();
        assert_eq!(dir, temp.path().join("loom").join("images"));
        unsafe {
            match prev_xdg {
                Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
                None => std::env::remove_var("XDG_CACHE_HOME"),
            }
            if let Some(v) = prev_lcd {
                std::env::set_var("LOOM_CACHE_DIR", v);
            }
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[serial]
    fn cache_dir_macos_default() {
        let temp = TempDir::new().unwrap();
        let prev_home = std::env::var_os("HOME");
        let prev_lcd = std::env::var_os("LOOM_CACHE_DIR");
        unsafe {
            std::env::remove_var("LOOM_CACHE_DIR");
            std::env::set_var("HOME", temp.path());
        }
        let dir = cache_dir().unwrap();
        assert_eq!(
            dir,
            temp.path()
                .join("Library")
                .join("Caches")
                .join("loom")
                .join("images")
        );
        unsafe {
            match prev_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
            match prev_lcd {
                Some(v) => std::env::set_var("LOOM_CACHE_DIR", v),
                None => {}
            }
        }
    }
}
