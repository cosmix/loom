//! Render the embedded, per-session planning plugin without reusing existing entries.

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, ensure, Context, Result};

use super::{GuidanceModels, SessionGuidanceKind};
use crate::assets::{Asset, CLAUDE_COMMANDS, PLAN_PLUGIN};
use crate::plan_session::kind::SessionDir;

type RenderedAsset = (&'static str, Cow<'static, str>);

pub(super) fn preflight(session_dir: &SessionDir) -> Result<()> {
    require_directory(session_dir.path())?;
    for path in [session_dir.plugin(), session_dir.brief()] {
        match fs::symlink_metadata(&path) {
            Ok(_) => bail!("guidance destination already exists: {}", path.display()),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("inspect {}", path.display()));
            }
        }
    }
    Ok(())
}

pub(super) fn write_plugin(
    kind: &SessionGuidanceKind,
    session_dir: &SessionDir,
    models: &GuidanceModels,
) -> Result<()> {
    let assets = render_assets(kind, models)?;
    let mut names = BTreeSet::new();
    for (name, _) in &assets {
        ensure!(
            !name.is_empty()
                && Path::new(name)
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "invalid embedded plugin path: {name}"
        );
        ensure!(
            names.insert(*name),
            "duplicate embedded plugin path: {name}"
        );
    }
    require_directory(session_dir.path())?;
    let root = session_dir.plugin();
    create_directory(&root)?;
    let mut created = BTreeSet::new();
    for (name, text) in assets {
        require_directory(session_dir.path())?;
        require_directory(&root)?;
        create_parents(&root, Path::new(name), &mut created)?;
        write_new(&root.join(name), text.as_bytes())?;
    }
    Ok(())
}

pub(super) fn write_brief(session_dir: &SessionDir, text: &str) -> Result<()> {
    require_directory(session_dir.path())?;
    write_new(&session_dir.brief(), text.as_bytes())
}

fn render_assets(
    kind: &SessionGuidanceKind,
    models: &GuidanceModels,
) -> Result<Vec<RenderedAsset>> {
    let manifest = ".claude-plugin/plugin.json";
    let mut assets = vec![(manifest, Cow::Borrowed(embedded(PLAN_PLUGIN, manifest)?))];
    match kind {
        SessionGuidanceKind::Pressure { .. } => {
            for (key, path) in [
                ("pressure.md", "commands/pressure.md"),
                ("address.md", "commands/address.md"),
            ] {
                assets.push((path, Cow::Borrowed(embedded(CLAUDE_COMMANDS, key)?)));
            }
        }
        SessionGuidanceKind::Authoring(_) | SessionGuidanceKind::Summary { .. } => {
            let prefix = match kind {
                SessionGuidanceKind::Authoring(_) => "skills/",
                _ => "skills/summaries/",
            };
            for &(path, text) in PLAN_PLUGIN {
                if path.starts_with(prefix) {
                    assets.push((path, Cow::Borrowed(text)));
                }
            }
            for (path, settings) in [
                ("agents/summary.md", &models.document_agent),
                ("agents/overview.md", &models.overview_agent),
            ] {
                let text = rewrite_agent(
                    embedded(PLAN_PLUGIN, path)?,
                    &settings.model,
                    &settings.effort,
                )
                .with_context(|| format!("render embedded {path}"))?;
                assets.push((path, Cow::Owned(text)));
            }
        }
    }
    Ok(assets)
}

fn embedded(assets: &[Asset], key: &str) -> Result<&'static str> {
    assets
        .iter()
        .find_map(|&(path, text)| (path == key).then_some(text))
        .with_context(|| format!("missing embedded plugin asset: {key}"))
}

fn rewrite_agent(source: &str, model: &str, effort: &str) -> Result<String> {
    let lines: Vec<_> = source.split_inclusive('\n').collect();
    ensure!(
        lines
            .first()
            .is_some_and(|line| line.trim_end_matches(['\r', '\n']) == "---"),
        "agent has no opening frontmatter delimiter"
    );
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(index, line)| (line.trim_end_matches(['\r', '\n']) == "---").then_some(index))
        .context("agent has no closing frontmatter delimiter")?;
    for key in ["model:", "effort:"] {
        ensure!(
            lines[1..end]
                .iter()
                .filter(|line| line.starts_with(key))
                .count()
                == 1,
            "agent frontmatter must contain exactly one {key} line"
        );
    }
    let mut rendered = String::with_capacity(source.len());
    for (index, line) in lines.into_iter().enumerate() {
        let replacement = if index > 0 && index < end {
            if line.starts_with("model:") {
                Some(("model", model))
            } else if line.starts_with("effort:") {
                Some(("effort", effort))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((key, value)) = replacement {
            rendered.push_str(&replace_line(line, key, value));
        } else {
            rendered.push_str(line);
        }
    }
    Ok(rendered)
}

fn replace_line(line: &str, key: &str, value: &str) -> String {
    let ending = if line.ends_with("\r\n") {
        "\r\n"
    } else if line.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    format!("{key}: {value}{ending}")
}

fn require_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect guidance directory {}", path.display()))?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "guidance directory must be a directory, not a symlink: {}",
        path.display()
    );
    Ok(())
}

fn create_directory(path: &Path) -> Result<()> {
    DirBuilder::new()
        .mode(0o700)
        .create(path)
        .with_context(|| format!("create guidance directory {}", path.display()))
}

fn create_parents(root: &Path, relative: &Path, created: &mut BTreeSet<PathBuf>) -> Result<()> {
    let parent = relative.parent().context("plugin asset has no parent")?;
    let mut path = root.to_path_buf();
    for component in parent.components() {
        path.push(component);
        if created.contains(&path) {
            require_directory(&path)?;
        } else {
            create_directory(&path)?;
            created.insert(path.clone());
        }
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("create guidance file {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write guidance file {}", path.display()))
}
