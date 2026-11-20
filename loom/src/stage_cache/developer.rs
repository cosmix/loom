//! The macOS developer directory and SDK, so confined builds run the real
//! tools instead of the `/usr/bin` xcrun shims. A confined command cannot use
//! xcrun's cache database (it stays denied: a writable one would let the
//! command plant tool paths for the operator's own builds), so every shim
//! call would otherwise start an uncached `xcodebuild` lookup.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

/// Upper bound for one unconfined lookup; a cold `xcrun` can take seconds.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(10);

/// The selected Xcode or Command Line Tools developer directory and the macOS SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeveloperTools {
    pub developer_dir: PathBuf,
    pub sdk_root: PathBuf,
}

impl DeveloperTools {
    /// Look the tools up once per process, unconfined, where xcrun's cache
    /// works. `None` off macOS, or when either lookup fails or names a path
    /// that is not an existing directory.
    pub fn detect() -> Option<DeveloperTools> {
        static TOOLS: OnceLock<Option<DeveloperTools>> = OnceLock::new();
        if !cfg!(target_os = "macos") {
            return None;
        }
        TOOLS
            .get_or_init(|| {
                Some(DeveloperTools {
                    developer_dir: lookup("/usr/bin/xcode-select", &["-p"])?,
                    sdk_root: lookup("/usr/bin/xcrun", &["--sdk", "macosx", "--show-sdk-path"])?,
                })
            })
            .clone()
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
