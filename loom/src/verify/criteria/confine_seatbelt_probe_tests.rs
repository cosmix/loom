//! Diagnostics for a failing Seatbelt ancestor test, computed only when its
//! assertion fails: the exact profile the confined run was given, and whether
//! `test -e` sees the manifest above the tree under that profile with each
//! candidate deny rule appended, and under a permissive base with the same
//! rule. A single macOS run then shows which rule form the kernel honors.

use std::path::Path;
use std::process::Command;

use crate::models::stage::CommandConfinement;
use crate::verify::criteria::confine::prepare_confined;
use crate::verify::criteria::CommandSpec;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// The profile, macOS release and probe results for `spec` confined to `tree`.
pub(super) fn report(spec: &CommandSpec, tree: &Path) -> String {
    let profile = match confined_profile(spec, tree) {
        Ok(profile) => profile,
        Err(error) => return format!("profile unavailable: {error:#}"),
    };
    let parent = tree.parent().expect("the fixture tree has a parent");
    let mut report = format!(
        "macOS: {}\nprofile: {profile}\nprobes (`test -e` exit 0 = file visible):\n",
        command_output(Command::new("sw_vers").arg("-productVersion"))
    );
    for target in [parent.join("Cargo.toml"), parent.join(".cargo/config.toml")] {
        report.push_str(&probe("real profile", &profile, "", &target));
    }
    let manifest = parent.join("Cargo.toml");
    for rule in candidate_rules(&manifest) {
        report.push_str(&probe("real profile +", &profile, &rule, &manifest));
        report.push_str(&probe(
            "allow default +",
            "(version 1)(allow default)",
            &rule,
            &manifest,
        ));
    }
    report
}

/// The `-p` argument of the command the confined run spawns, built through
/// the same preparation as `run_spec_with_timeout`.
fn confined_profile(spec: &CommandSpec, tree: &Path) -> anyhow::Result<String> {
    let prepared = prepare_confined(spec, Some(tree), None, CommandConfinement::Confined, None)?;
    let wrapped = prepared
        .wrapped
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("confined preparation produced no wrapped command"))?;
    let args: Vec<String> = wrapped
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let at = args
        .iter()
        .position(|arg| arg == "-p")
        .ok_or_else(|| anyhow::anyhow!("no -p argument in {args:?}"))?;
    Ok(args[at + 1].clone())
}

/// Deny rules for `file` in each form under suspicion: the wildcard alone,
/// named operations, the `/var` alias, the data-volume firmlink path, and a
/// regex that matches whatever prefix the kernel reports.
fn candidate_rules(file: &Path) -> Vec<String> {
    let canonical = file.to_string_lossy().into_owned();
    let alias = canonical
        .strip_prefix("/private")
        .unwrap_or(&canonical)
        .to_owned();
    let firmlink = format!("/System/Volumes/Data{canonical}");
    let quoted = |path: &str| serde_json::to_string(path).unwrap();
    let name = file.file_name().unwrap().to_string_lossy();
    let parent = file
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy();
    let regex = format!("#\"/{}/{}$\"", regex_escape(&parent), regex_escape(&name));
    vec![
        format!("(deny file-read* (literal {}))", quoted(&canonical)),
        format!("(deny file-read-metadata (literal {}))", quoted(&canonical)),
        format!(
            "(deny file-read-metadata file-test-existence (literal {}))",
            quoted(&canonical)
        ),
        format!("(deny file-read-metadata (literal {}))", quoted(&alias)),
        format!("(deny file-read-metadata (literal {}))", quoted(&firmlink)),
        format!("(deny file-read-metadata (regex {regex}))"),
    ]
}

/// `text` with its dots escaped: fixture names hold only alphanumerics and dots.
fn regex_escape(text: &str) -> String {
    text.replace('.', "\\.")
}

/// One line: whether `test -e target` succeeds under `base` + `rule`.
fn probe(label: &str, base: &str, rule: &str, target: &Path) -> String {
    let mut command = Command::new(SANDBOX_EXEC);
    command
        .arg("-p")
        .arg(format!("{base}{rule}"))
        .args([
            "/bin/sh",
            "-c",
            "test -e \"$1\"; echo \"test-e=$?\"",
            "probe",
        ])
        .arg(target)
        .current_dir("/");
    format!(
        "  [{label} {rule}] {}: {}\n",
        target.display(),
        command_output(&mut command)
    )
}

fn command_output(command: &mut Command) -> String {
    match command.output() {
        Ok(output) => format!(
            "status={} stdout={:?} stderr={:?}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
        Err(error) => format!("spawn failed: {error}"),
    }
}
