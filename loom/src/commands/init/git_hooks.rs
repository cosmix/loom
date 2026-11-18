//! `loom init`'s git hooks: the pre-commit hook that keeps orchestration
//! state out of commits and the reference-transaction hook that attests
//! operator moves of a guarded target.

use colored::Colorize;
use std::path::Path;

use crate::git::hooks::{install_reference_transaction_hook, HookInstall};
use crate::git::install_pre_commit_hook;

/// Install both hooks in the directory git runs hooks from, one status line each.
/// A failed install, a `core.hooksPath` loom must not write to among the causes,
/// prints a warning and does not fail `init`.
pub(super) fn install_git_hooks(repo_root: &Path) {
    install_pre_commit(repo_root);
    install_reference_transaction(repo_root);
}

fn install_pre_commit(repo_root: &Path) {
    match install_pre_commit_hook(repo_root) {
        Ok(true) => println!("  {} Git pre-commit hook installed", "✓".green().bold()),
        Ok(false) => println!(
            "  {} Git pre-commit hook {} up to date",
            "✓".green().bold(),
            "already".dimmed()
        ),
        Err(e) => println!(
            "  {} Git pre-commit hook installation failed: {}",
            "!".yellow().bold(),
            e.to_string().dimmed()
        ),
    }
}

fn install_reference_transaction(repo_root: &Path) {
    match install_reference_transaction_hook(repo_root) {
        Ok(HookInstall::Installed) => println!(
            "  {} Git reference-transaction hook installed",
            "✓".green().bold()
        ),
        Ok(HookInstall::UpToDate) => println!(
            "  {} Git reference-transaction hook {} up to date",
            "✓".green().bold(),
            "already".dimmed()
        ),
        Ok(HookInstall::ForeignHookPresent) => println!(
            "  {} The reference-transaction hook belongs to another tool; loom cannot \
             attest operator moves of the target, so the target guard runs without \
             attestation",
            "!".yellow().bold()
        ),
        Err(e) => println!(
            "  {} Git reference-transaction hook installation failed: {}",
            "!".yellow().bold(),
            e.to_string().dimmed()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::target_guard::HOOK_MARKER;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn install_git_hooks_writes_the_reference_transaction_hook() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join(".git")).unwrap();

        install_git_hooks(temp.path());

        let hook = temp.path().join(".git/hooks/reference-transaction");
        let script = std::fs::read_to_string(&hook).unwrap();
        assert!(script.contains(HOOK_MARKER));
        let mode = std::fs::metadata(&hook).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}
