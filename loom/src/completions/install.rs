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
        Shell::Zsh => {
            let home = home_dir()?;
            Ok(home.join(".zfunc/_loom"))
        }
        Shell::Fish => {
            let config_dir = std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir().unwrap_or_default().join(".config"));
            Ok(config_dir.join("fish/completions/loom.fish"))
        }
    }
}

/// Install completions to the system location
pub fn install(shell: super::generator::Shell) -> Result<()> {
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

    print_post_install(shell, &path);

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

fn print_post_install(shell: super::generator::Shell, path: &Path) {
    use super::generator::Shell;

    match shell {
        Shell::Bash => {
            eprintln!("\nTo activate, add to ~/.bashrc:");
            eprintln!("  source {}", path.display());
            eprintln!("\nOr restart your shell.");
        }
        Shell::Zsh => {
            eprintln!("\nTo activate, ensure ~/.zfunc is in your fpath.");
            eprintln!("Add to ~/.zshrc (before compinit):");
            eprintln!("  fpath=(~/.zfunc $fpath)");
            eprintln!("  autoload -Uz compinit && compinit");
            eprintln!("\nOr restart your shell.");
        }
        Shell::Fish => {
            eprintln!("\nCompletions are active immediately in new fish sessions.");
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
    let stale_paths = completion_file_paths(home);

    let mut found = false;
    for path in &stale_paths {
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

fn completion_file_paths(home: &Path) -> [PathBuf; 3] {
    [
        home.join(".local/share/bash-completion/completions/loom"),
        home.join(".zfunc/_loom"),
        home.join(".config/fish/completions/loom.fish"),
    ]
}
