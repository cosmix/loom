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
/// under `home` once both sides are canonicalised (an XDG variable pointing
/// outside `home` — including one that only escapes via a `..` component,
/// such as `XDG_DATA_HOME=$HOME/../../tmp/x` — is discarded, which keeps
/// today's behavior — the safe direction).
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
        if is_under_home(&path, home) && !candidates.contains(&path) {
            candidates.push(path);
        }
    }
    candidates
}

/// Whether `path` resolves under `home` once both are canonicalised.
///
/// A lexical `path.starts_with(home)` accepts a `..`-laden path whose
/// components merely begin with `home`'s, even though it resolves somewhere
/// else entirely (`$HOME/../../tmp/x` "starts with" `$HOME` lexically while
/// resolving outside it). Canonicalising both sides first closes that gap. A
/// candidate that does not exist yet — the common case, since most shells'
/// completion files are simply absent — fails to canonicalise and is
/// discarded here rather than trusted; the caller's own `is_file()` check
/// would have discarded it anyway.
///
/// Canonicalising also discards a candidate that is itself a symlink into a
/// directory outside `home` - the pre-canonicalisation lexical check would
/// have refreshed it - and this function only ever runs on the XDG
/// candidate; the default (non-XDG) path from `default_install_path` is
/// never checked against `home` at all, so a symlinked
/// `~/.local/share/.../loom` completion is still written through. Discarding
/// is the safe direction in both cases, so neither is fixed here.
fn is_under_home(path: &Path, home: &Path) -> bool {
    let (Ok(home), Ok(path)) = (home.canonicalize(), path.canonicalize()) else {
        return false;
    };
    path.starts_with(home)
}
