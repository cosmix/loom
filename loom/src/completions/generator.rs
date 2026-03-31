use anyhow::{anyhow, Result};
use std::str::FromStr;

use super::scripts;

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

/// Generate shell completion script and write to stdout
pub fn generate_completions(shell: Shell) {
    match shell {
        Shell::Bash => print!("{}", scripts::BASH_COMPLETION),
        Shell::Zsh => print!("{}", scripts::ZSH_COMPLETION),
        Shell::Fish => print!("{}", scripts::FISH_COMPLETION),
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
