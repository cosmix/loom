//! The macOS developer directory and SDK, so confined builds run the real
//! tools instead of the `/usr/bin` xcrun shims. A confined command cannot use
//! xcrun's cache database (it stays denied: a writable one would let the
//! command plant tool paths for the operator's own builds), so every shim
//! call would otherwise start an uncached `xcodebuild` lookup.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Upper bound for one unconfined lookup; a cold `xcrun` can take seconds.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a failed lookup is believed before the next one: a transient
/// failure (a busy `xcrun`, an Xcode install in progress) must not disable
/// the real tools for the life of a long-running daemon.
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// The selected Xcode or Command Line Tools developer directory and the macOS SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeveloperTools {
    pub developer_dir: PathBuf,
    pub sdk_root: PathBuf,
}

/// What [`DeveloperTools::detect`] remembers: a found answer for good, a
/// failure until [`RETRY_AFTER`] has passed.
struct Detection {
    found: Option<DeveloperTools>,
    failed_at: Option<Instant>,
}

static CACHE: Mutex<Detection> = Mutex::new(Detection {
    found: None,
    failed_at: None,
});

impl DeveloperTools {
    /// Look the tools up, unconfined, where xcrun's cache works. A found
    /// answer is kept for the process; a failed lookup is retried after
    /// `RETRY_AFTER`. `None` off macOS, or when either lookup fails or
    /// names a path that is not an existing directory.
    pub fn detect() -> Option<DeveloperTools> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        detect_with(&CACHE, Instant::now(), lookup_tools)
    }
}

fn lookup_tools() -> Option<DeveloperTools> {
    Some(DeveloperTools {
        developer_dir: lookup("/usr/bin/xcode-select", &["-p"])?,
        sdk_root: lookup("/usr/bin/xcrun", &["--sdk", "macosx", "--show-sdk-path"])?,
    })
}

/// `cache`'s answer, or `lookup`'s when none is kept. The lock is held across
/// the lookup so one `xcrun` runs at a time.
fn detect_with(
    cache: &Mutex<Detection>,
    now: Instant,
    lookup: impl FnOnce() -> Option<DeveloperTools>,
) -> Option<DeveloperTools> {
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(found) = &cache.found {
        return Some(found.clone());
    }
    if cache
        .failed_at
        .is_some_and(|failed| now.saturating_duration_since(failed) < RETRY_AFTER)
    {
        return None;
    }
    match lookup() {
        Some(found) => {
            cache.found = Some(found.clone());
            Some(found)
        }
        None => {
            cache.failed_at = Some(now);
            None
        }
    }
}

/// Run `program` with only `PATH=/usr/bin:/bin` and read the directory it prints.
fn lookup(program: &str, args: &[&str]) -> Option<PathBuf> {
    let mut command = Command::new(program);
    command.env_clear().env("PATH", "/usr/bin:/bin").args(args);
    let output = crate::process::run_bounded(&mut command, LOOKUP_TIMEOUT)
        .ok()?
        .completed()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    (path.is_absolute() && path.is_dir()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_failed_lookup_is_retried_after_the_throttle() {
        let cache = Mutex::new(Detection {
            found: None,
            failed_at: None,
        });
        let tools = DeveloperTools {
            developer_dir: PathBuf::from("/dev-dir"),
            sdk_root: PathBuf::from("/sdk"),
        };
        let calls = Cell::new(0);
        let lookup = || {
            calls.set(calls.get() + 1);
            (calls.get() > 1).then(|| tools.clone())
        };
        let t = Instant::now();

        assert_eq!(detect_with(&cache, t, lookup), None);
        assert_eq!(calls.get(), 1);
        assert_eq!(
            detect_with(&cache, t + Duration::from_secs(30), lookup),
            None
        );
        assert_eq!(calls.get(), 1, "inside the throttle the lookup is skipped");
        assert_eq!(
            detect_with(&cache, t + Duration::from_secs(61), lookup),
            Some(tools.clone())
        );
        assert_eq!(calls.get(), 2);
        assert_eq!(
            detect_with(&cache, t + Duration::from_secs(62), lookup),
            Some(tools)
        );
        assert_eq!(calls.get(), 2, "a found answer is cached");
    }
}
