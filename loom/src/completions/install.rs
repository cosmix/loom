use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};

/// Detect the user's shell from $SHELL environment variable
pub fn detect_shell() -> Result<super::generator::Shell> {
    let shell_path = std::env::var("SHELL")
        .context("$SHELL not set — specify shell explicitly: loom completions --install bash")?;

    let shell_name = std::path::Path::new(&shell_path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow!("Could not parse $SHELL: {shell_path}"))?;

    shell_name.parse()
}

fn home_dir() -> Result<PathBuf> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .context("Could not determine home directory: $HOME not set")
}

/// Get the appropriate installation path for a shell's completions
pub fn install_path(shell: super::generator::Shell) -> Result<PathBuf> {
    use super::generator::Shell;

    match shell {
        Shell::Bash => {
            let data_dir = std::env::var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir().unwrap_or_default().join(".local/share"));
            Ok(data_dir.join("bash-completion/completions/loom"))
        }
        Shell::Zsh => zsh_install_path(),
        Shell::Fish => {
            let config_dir = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir().unwrap_or_default().join(".config"));
            Ok(config_dir.join("fish/completions/loom.fish"))
        }
    }
}

/// Find the best zsh install path: prefer an existing writable fpath dir,
/// fall back to ~/.zfunc (and configure .zshrc to include it).
fn zsh_install_path() -> Result<PathBuf> {
    // Check existing fpath dirs for a writable one
    if let Ok(fpath) = std::env::var("FPATH") {
        for dir in fpath.split(':') {
            let p = Path::new(dir);
            if p.is_dir() && is_writable(p) {
                return Ok(p.join("_loom"));
            }
        }
    }

    // No writable fpath dir found — use ~/.zfunc (will be configured in .zshrc)
    let home = home_dir()?;
    Ok(home.join(".zfunc/_loom"))
}

/// Check if a directory is writable by attempting to create a temp file.
fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".loom_write_probe");
    if std::fs::write(&probe, b"").is_ok() {
        let _ = std::fs::remove_file(&probe);
        true
    } else {
        false
    }
}

/// Install completions to the system location
pub fn install(shell: super::generator::Shell) -> Result<()> {
    use super::generator::Shell;

    let path = install_path(shell)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    let content = completion_content(shell);

    if path.exists() {
        eprintln!("Updating existing completions at: {}", path.display());
    }

    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write completions to: {}", path.display()))?;

    eprintln!(
        "Installed {} completions to: {}",
        shell_name(shell),
        path.display()
    );

    let home = home_dir()?;
    match shell {
        Shell::Zsh => {
            if path.starts_with(home.join(".zfunc")) {
                ensure_zshrc_fpath(&home)?;
            }
        }
        Shell::Bash => {
            ensure_bashrc_completion(&home, &path)?;
        }
        Shell::Fish => {}
    }

    print_post_install(shell, &path);

    Ok(())
}

const RC_MARKER: &str = "# loom completions";

/// Ensure ~/.bashrc sources the completion file if bash-completion isn't auto-loading it.
fn ensure_bashrc_completion(home: &Path, completion_path: &Path) -> Result<()> {
    let bashrc = home.join(".bashrc");

    let existing = if bashrc.exists() {
        std::fs::read_to_string(&bashrc)
            .with_context(|| format!("Failed to read {}", bashrc.display()))?
    } else {
        String::new()
    };

    if existing.contains(RC_MARKER) {
        return Ok(());
    }

    // If bash-completion is loaded, the XDG dir is auto-sourced — no edit needed
    if existing.contains("bash_completion") || existing.contains("bash-completion") {
        return Ok(());
    }

    let snippet = format!(
        "\n{RC_MARKER}\n[ -f {} ] && source {}\n",
        completion_path.display(),
        completion_path.display()
    );

    let updated = format!("{existing}{snippet}");
    std::fs::write(&bashrc, updated)
        .with_context(|| format!("Failed to write {}", bashrc.display()))?;

    eprintln!("Configured ~/.bashrc to source loom completions.");
    Ok(())
}

/// Ensure ~/.zshrc has fpath and compinit configured for ~/.zfunc.
fn ensure_zshrc_fpath(home: &Path) -> Result<()> {
    let zshrc = home.join(".zshrc");

    let existing = if zshrc.exists() {
        std::fs::read_to_string(&zshrc)
            .with_context(|| format!("Failed to read {}", zshrc.display()))?
    } else {
        String::new()
    };

    // Already configured (by us or manually)
    if existing.contains(RC_MARKER) {
        return Ok(());
    }

    // Check if fpath already includes ~/.zfunc
    if existing.contains("~/.zfunc") || existing.contains("$HOME/.zfunc") {
        eprintln!("~/.zfunc already referenced in .zshrc, skipping configuration.");
        return Ok(());
    }

    let snippet =
        format!("\n{RC_MARKER}\nfpath=(~/.zfunc $fpath)\nautoload -Uz compinit && compinit\n");

    // Append before any existing compinit call if possible, otherwise just append
    let updated = if let Some(pos) = existing.find("autoload -Uz compinit") {
        // Insert fpath line before the existing compinit
        let (before, after) = existing.split_at(pos);
        format!("{before}{RC_MARKER}\nfpath=(~/.zfunc $fpath)\n{after}")
    } else {
        format!("{existing}{snippet}")
    };

    std::fs::write(&zshrc, updated)
        .with_context(|| format!("Failed to write {}", zshrc.display()))?;

    eprintln!("Configured ~/.zshrc to load completions from ~/.zfunc.");

    Ok(())
}

fn completion_content(shell: super::generator::Shell) -> &'static str {
    use super::generator::Shell;

    match shell {
        Shell::Bash => super::scripts::BASH_COMPLETION,
        Shell::Zsh => super::scripts::ZSH_COMPLETION,
        Shell::Fish => super::scripts::FISH_COMPLETION,
    }
}

fn shell_name(shell: super::generator::Shell) -> &'static str {
    use super::generator::Shell;

    match shell {
        Shell::Bash => "bash",
        Shell::Zsh => "zsh",
        Shell::Fish => "fish",
    }
}

fn print_post_install(shell: super::generator::Shell, _path: &Path) {
    use super::generator::Shell;

    match shell {
        Shell::Fish => {
            eprintln!("\nCompletions are active immediately in new fish sessions.");
        }
        _ => {
            eprintln!("\nRestart your shell to activate completions.");
        }
    }
}

/// Check for outdated completion setups and print migration instructions
pub fn check_migration() -> Result<()> {
    let home = home_dir()?;

    let mut found_issues = false;

    found_issues |= check_rc_files(&home);
    found_issues |= check_stale_files(&home);

    if !found_issues {
        print_current_status(&home);
    }

    Ok(())
}

fn check_rc_files(home: &Path) -> bool {
    let rc_files = [
        (home.join(".bashrc"), "bash"),
        (home.join(".bash_profile"), "bash"),
        (home.join(".zshrc"), "zsh"),
        (home.join(".zprofile"), "zsh"),
    ];

    let mut found = false;
    for (rc_path, shell_name) in &rc_files {
        if !rc_path.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(rc_path) else {
            continue;
        };
        if content.contains("eval \"$(loom completions") {
            found = true;
            println!("Found old completion setup in {}:", rc_path.display());
            println!("  eval \"$(loom completions {shell_name})\"");
            println!();
            println!("  This still works, but consider switching to file-based installation:");
            println!("    1. Remove the eval line from {}", rc_path.display());
            println!("    2. Run: loom completions --install");
            println!("    3. Follow the post-install instructions");
            println!();
            println!("  Benefits of file-based installation:");
            println!("    - Faster shell startup (no subprocess on every shell open)");
            println!("    - Completions available even if loom binary is not in PATH yet");
            println!();
        }
    }
    found
}

fn check_stale_files(home: &Path) -> bool {
    let mut candidates = completion_file_paths(home);

    // Also scan all zsh fpath dirs for stale _loom files
    if let Ok(fpath) = std::env::var("FPATH") {
        for dir in fpath.split(':') {
            let path = PathBuf::from(dir).join("_loom");
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
    }

    let mut found = false;
    for path in &candidates {
        if !path.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        if content.contains("_clap_complete") || content.contains("clap_complete") {
            found = true;
            println!("Found outdated completion file: {}", path.display());
            println!("  This file was generated by an older version of loom.");
            println!("  Update it with: loom completions --install");
            println!();
        }
    }
    found
}

fn print_current_status(home: &Path) {
    println!("No migration issues found.");
    println!();
    println!("Current completion setup:");

    let paths = completion_file_paths(home);
    let mut any_installed = false;
    for path in &paths {
        if path.exists() {
            println!("  Installed: {}", path.display());
            any_installed = true;
        }
    }

    if !any_installed {
        println!("  No file-based completions installed.");
        println!("  Install with: loom completions --install");
    }
}

fn completion_file_paths(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local/share/bash-completion/completions/loom"),
        home.join(".zfunc/_loom"),
        home.join(".config/fish/completions/loom.fish"),
    ]
}
