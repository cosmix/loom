//! Capability probe failures at the session launch boundary.

use super::capsule::probe_capsule_support;
use std::os::unix::fs::PermissionsExt;

#[test]
fn failed_and_hanging_help_probes_refuse_with_diagnostics() {
    let temp = tempfile::TempDir::new().unwrap();
    let executable = temp.path().join("claude");
    for (body, expected) in [
        ("echo unavailable >&2; exit 7", "unavailable"),
        ("sleep 30", "timed out"),
    ] {
        std::fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let error = probe_capsule_support(&executable).unwrap_err();
        let diagnostic = format!("{error:#}").to_lowercase();
        assert!(diagnostic.contains(expected), "{diagnostic}");
    }
}

#[test]
fn partial_help_probe_reports_missing_required_flags() {
    let temp = tempfile::TempDir::new().unwrap();
    let executable = temp.path().join("claude");
    std::fs::write(
        &executable,
        "#!/bin/sh\necho '--settings --strict-mcp-config'\n",
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let error = probe_capsule_support(&executable).unwrap_err().to_string();
    assert!(error.contains("--setting-sources"), "{error}");
}
