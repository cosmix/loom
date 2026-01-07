use anyhow::{anyhow, Result};
use clap::Command;
use clap_complete::{generate, shells};
use std::io;
use std::str::FromStr;

/// Supported shell types for completion generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl FromStr for Shell {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "bash" => Ok(Shell::Bash),
            "zsh" => Ok(Shell::Zsh),
            "fish" => Ok(Shell::Fish),
            _ => Err(anyhow!(
                "Unsupported shell: {s}. Supported shells: bash, zsh, fish"
            )),
        }
    }
}

impl From<Shell> for shells::Bash {
    fn from(_: Shell) -> Self {
        shells::Bash
    }
}

impl From<Shell> for shells::Zsh {
    fn from(_: Shell) -> Self {
        shells::Zsh
    }
}

impl From<Shell> for shells::Fish {
    fn from(_: Shell) -> Self {
        shells::Fish
    }
}

/// Generate shell completion script and write to stdout
///
/// # Arguments
///
/// * `cmd` - The clap Command to generate completions for
/// * `shell` - Target shell type
///
/// # Example
///
/// ```no_run
/// use clap::Command;
/// use loom::completions::generator::{Shell, generate_completions};
/// use std::str::FromStr;
///
/// let mut cmd = Command::new("loom");
/// let shell = Shell::from_str("bash").unwrap();
/// generate_completions(&mut cmd, shell);
/// ```
pub fn generate_completions(cmd: &mut Command, shell: Shell) {
    let bin_name = cmd.get_name().to_string();

    match shell {
        Shell::Bash => generate(shells::Bash, cmd, bin_name, &mut io::stdout()),
        Shell::Zsh => generate(shells::Zsh, cmd, bin_name, &mut io::stdout()),
        Shell::Fish => generate(shells::Fish, cmd, bin_name, &mut io::stdout()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_from_str_valid() {
        assert_eq!(Shell::from_str("bash").unwrap(), Shell::Bash);
        assert_eq!(Shell::from_str("Bash").unwrap(), Shell::Bash);
        assert_eq!(Shell::from_str("BASH").unwrap(), Shell::Bash);

        assert_eq!(Shell::from_str("zsh").unwrap(), Shell::Zsh);
        assert_eq!(Shell::from_str("Zsh").unwrap(), Shell::Zsh);
        assert_eq!(Shell::from_str("ZSH").unwrap(), Shell::Zsh);

        assert_eq!(Shell::from_str("fish").unwrap(), Shell::Fish);
        assert_eq!(Shell::from_str("Fish").unwrap(), Shell::Fish);
        assert_eq!(Shell::from_str("FISH").unwrap(), Shell::Fish);
    }

    #[test]
    fn test_shell_from_str_invalid() {
        assert!(Shell::from_str("powershell").is_err());
        assert!(Shell::from_str("cmd").is_err());
        assert!(Shell::from_str("").is_err());
        assert!(Shell::from_str("invalid").is_err());
    }

    #[test]
    fn test_shell_from_str_error_message() {
        let result = Shell::from_str("powershell");
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(err_msg.contains("Unsupported shell"));
        assert!(err_msg.contains("powershell"));
        assert!(err_msg.contains("bash, zsh, fish"));
    }
}
