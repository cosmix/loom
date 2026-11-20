//! Linux overlays of the host's uv and Bun caches beneath a command's private
//! caches (see `confine_scratch.rs`), used only when no stage cache replaces them.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The host caches [`cache_lowers`] selects that `scratch`'s filesystem can
/// overlay; each unsupported one is logged and left out.
pub(super) fn supported_lowers(command: &Command, scratch: &Path) -> Vec<(&'static str, PathBuf)> {
    cache_lowers(command)
        .into_iter()
        .filter(|(name, source)| {
            let supported = overlay_supported(scratch, name, source);
            if !supported {
                tracing::warn!(
                    cache = name,
                    "cache overlay unavailable; using empty private cache"
                );
            }
            supported
        })
        .collect()
}

pub(super) fn cache_lowers(command: &Command) -> Vec<(&'static str, PathBuf)> {
    let value = |key: &str| super::super::host::environment_path(command, key);
    // Reuse only roots already exposed by the host policy. An arbitrary
    // command-provided cache path must not grant access to private host files.
    let Some(home) = value("HOME") else {
        return Vec::new();
    };
    let uv = home.join(".cache/uv");
    let bun = home.join(".bun/install/cache");
    let uv_requested =
        value("UV_CACHE_DIR").or_else(|| value("XDG_CACHE_HOME").map(|p| p.join("uv")));
    let bun_requested = value("BUN_INSTALL_CACHE_DIR");
    [
        ("uv-cache", uv, uv_requested),
        ("bun-cache", bun, bun_requested),
    ]
    .into_iter()
    .filter_map(|(name, default, requested)| {
        let source = default.canonicalize().ok().filter(|p| p.is_dir())?;
        if requested.is_some_and(|p| p.canonicalize().ok().as_ref() != Some(&source)) {
            tracing::warn!(
                cache = name,
                "custom cache is outside the allowed host roots; using empty private cache"
            );
            return None;
        }
        Some((name, source))
    })
    .collect()
}

/// Test this lower and upper filesystem together; overlay support varies by mount.
/// The probe reads the cache but its private namespace never writes to it.
pub(super) fn overlay_supported(scratch: &Path, name: &str, source: &Path) -> bool {
    let Ok(bwrap) = which::which("bwrap") else {
        return false;
    };
    let root = scratch.join(format!("overlay-{name}"));
    let upper = root.join("upper");
    let work = root.join("work");
    let target = scratch.join(name);
    if [&upper, &work, &target]
        .into_iter()
        .any(|p| std::fs::create_dir_all(p).is_err())
    {
        return false;
    }
    use wait_timeout::ChildExt;
    let child = Command::new(bwrap)
        .args([
            "--die-with-parent",
            "--unshare-all",
            "--ro-bind",
            "/",
            "/",
            "--overlay-src",
        ])
        .arg(source)
        .arg("--overlay")
        .arg(&upper)
        .arg(&work)
        .arg(&target)
        .args(["--", "/usr/bin/true"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    let supported = match child.wait_timeout(std::time::Duration::from_secs(3)) {
        Ok(Some(status)) => status.success(),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            false
        }
    };
    super::reclaim::make_removable(&work);
    supported
}
