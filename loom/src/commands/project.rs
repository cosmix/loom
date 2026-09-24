//! `loom project detect`: the packages project discovery finds in a checkout,
//! each with its language kinds, test runner and language skills.

use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde::Serialize;

use crate::skills::project::{PackageDetail, ProjectProfile};

/// The `--json` document. Field order is the wire order.
#[derive(Serialize)]
struct Detection {
    root: PathBuf,
    truncated: bool,
    packages: Vec<PackageDetail>,
}

pub fn execute(path: Option<PathBuf>, json: bool) -> Result<()> {
    let start = match path {
        Some(path) => path,
        None => std::env::current_dir().context("reading the current directory")?,
    };
    // Discovery walks ancestors looking for the checkout root, so a mistyped
    // path would silently scan whichever enclosing checkout it lands in.
    let start = start
        .canonicalize()
        .with_context(|| format!("resolving {}", start.display()))?;
    ensure!(start.is_dir(), "{} is not a directory", start.display());

    let profile = ProjectProfile::discover(&start);
    let packages = profile.package_details();
    let detection = Detection {
        root: profile.root,
        truncated: profile.truncated,
        packages,
    };
    if json {
        println!("{}", serde_json::to_string(&detection)?);
    } else {
        println!("{}", render(&detection));
    }
    Ok(())
}

/// The human report: the checkout root, `truncated`, then one line per package.
fn render(detection: &Detection) -> String {
    let mut lines = vec![
        format!("root: {}", detection.root.display()),
        format!("truncated: {}", detection.truncated),
    ];
    lines.extend(detection.packages.iter().map(package_line));
    lines.join("\n")
}

fn package_line(package: &PackageDetail) -> String {
    // A package at the checkout root has an empty relative path.
    let path = if package.path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        &package.path
    };
    format!(
        "{}  kinds={}  runner={}  skills={}",
        path.display(),
        package.kinds.join(","),
        package.runner.unwrap_or("unsupported"),
        package.skills.join(","),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Detection {
        Detection {
            root: PathBuf::from("/repo"),
            truncated: false,
            packages: vec![
                PackageDetail {
                    path: PathBuf::from("loom"),
                    kinds: vec!["rust".to_string()],
                    runner: Some("cargo-test"),
                    skills: vec!["loom-rust".to_string()],
                },
                // Infrastructure at the checkout root: no test-runner adapter.
                PackageDetail {
                    path: PathBuf::new(),
                    kinds: vec!["docker".to_string()],
                    runner: None,
                    skills: vec!["loom-docker".to_string()],
                },
            ],
        }
    }

    #[test]
    fn human_report_lists_root_truncation_and_each_package() {
        assert_eq!(
            render(&sample()),
            "root: /repo\n\
             truncated: false\n\
             loom  kinds=rust  runner=cargo-test  skills=loom-rust\n\
             .  kinds=docker  runner=unsupported  skills=loom-docker"
        );
    }

    #[test]
    fn json_report_is_compact_and_keeps_null_runner() {
        let expected = concat!(
            r#"{"root":"/repo","truncated":false,"packages":["#,
            r#"{"path":"loom","kinds":["rust"],"runner":"cargo-test","skills":["loom-rust"]},"#,
            r#"{"path":"","kinds":["docker"],"runner":null,"skills":["loom-docker"]}]}"#,
        );
        assert_eq!(serde_json::to_string(&sample()).unwrap(), expected);
    }
}
