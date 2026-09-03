use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Rewrite completion files already installed below `home` without creating any.
pub fn refresh_existing_in(home: &Path) -> Result<usize> {
    use super::super::generator::Shell;

    let mut refreshed = 0;
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        for path in refresh_candidates(home, shell) {
            if !path.is_file() {
                continue;
            }
            std::fs::write(&path, super::completion_content(shell))
                .with_context(|| format!("Failed to write completions to: {}", path.display()))?;
            refreshed += 1;
        }
    }
    Ok(refreshed)
}

/// Candidate completion file locations under `home` for `shell`: the default
/// path, plus for bash/fish an XDG-derived path when it actually resolves
/// under `home` (an XDG variable pointing outside `home` is discarded, which
/// keeps today's behavior — the safe direction).
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
        if path.starts_with(home) && !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    candidates
}
