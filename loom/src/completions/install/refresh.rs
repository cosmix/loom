use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Rewrite completion files already installed below `home` without creating any.
pub fn refresh_existing_in(home: &Path) -> Result<usize> {
    use super::super::generator::Shell;

    let home = match home.canonicalize() {
        Ok(home) => home,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error).context("Failed to resolve home directory"),
    };
    let mut refreshed = 0;
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        for path in refresh_candidates(&home, shell) {
            let Ok(resolved) = path.canonicalize() else {
                continue;
            };
            let Ok(relative) = resolved.strip_prefix(&home) else {
                continue;
            };
            if !resolved.is_file() {
                continue;
            }
            crate::fs::safe_replace::replace_no_follow(
                &home,
                relative,
                super::completion_content(shell).as_bytes(),
            )
            .with_context(|| format!("Failed to write completions to: {}", path.display()))?;
            refreshed += 1;
        }
    }
    Ok(refreshed)
}

/// Candidate locations are resolved and confined by the caller before placement.
///
/// zsh is intentionally left on its default `~/.zfunc/_loom` path only. Its
/// install-time equivalent, `zsh_install_path`, probes `FPATH` entries by
/// writing a probe file, and a writable fpath directory is typically a
/// system location outside the operator's home; this refresh must never
/// write outside `home`, so that probe is not reused here.
fn refresh_candidates(home: &Path, shell: super::super::generator::Shell) -> Vec<PathBuf> {
    use super::super::generator::Shell;

    let mut candidates = vec![super::default_install_path(home, shell)];
    let xdg = match shell {
        Shell::Bash => std::env::var_os("XDG_DATA_HOME")
            .map(|dir| PathBuf::from(dir).join("bash-completion/completions/loom")),
        Shell::Fish => std::env::var_os("XDG_CONFIG_HOME")
            .map(|dir| PathBuf::from(dir).join("fish/completions/loom.fish")),
        Shell::Zsh => None,
    };
    if let Some(path) = xdg {
        if !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    candidates
}
