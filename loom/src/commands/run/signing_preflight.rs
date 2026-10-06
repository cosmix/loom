//! Signing preflight for `loom run`.
//!
//! When `commit.gpgsign` is true the daemon signs every stage and merge
//! commit itself, without a terminal and in its own allowlisted environment.
//! A signer that works in the operator's shell but not there (a passphrase no
//! agent has cached, signing switched on only through variables the
//! allowlist drops) would block every stage at its first commit, so `loom
//! run` reads the setting and signs a test commit in both environments first.

use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::Path;

use crate::git::signing;

const CACHE_CAVEAT: &str = "gpg-agent forgets a cached passphrase 600 s after its last use \
     (default-cache-ttl; at most max-cache-ttl, 7200 s): raise both in gpg-agent.conf or sign \
     with an ssh-agent key, or stages block at their first commit.";

/// Refuse a run whose daemon could not sign. The operator's run sees this
/// process's environment, plus `GPG_TTY` when stdin is a terminal and it is
/// unset (so a terminal pinentry can prompt and the agent caches the
/// passphrase); the daemon's run sees exactly what the daemon child receives.
/// Both carry the signing environment (`signing::current`), which a
/// foreground run has already taken out of its own environment.
pub(super) fn require_signing(repo_root: &Path) -> Result<()> {
    let signing_env: Vec<(OsString, OsString)> = signing::current()
        .env_pairs()
        .into_iter()
        .map(|(name, value)| (name.into(), value.to_owned()))
        .collect();
    let mut operator: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    operator.extend(signing_env.iter().cloned());
    if let Some(tty) = gpg_tty(&operator) {
        operator.push(("GPG_TTY".into(), tty));
    }
    let mut daemon = crate::daemon::daemon_environment_pairs();
    daemon.extend(signing_env);
    require_signing_with(repo_root, &operator, &daemon)
}

/// The terminal on stdin, when stdin is one and `env` has no `GPG_TTY`.
fn gpg_tty(env: &[(OsString, OsString)]) -> Option<OsString> {
    let stdin = std::io::stdin();
    if env.iter().any(|(name, _)| name == "GPG_TTY") || !stdin.is_terminal() {
        return None;
    }
    nix::unistd::ttyname(stdin).ok().map(Into::into)
}

/// [`require_signing`] with both environments given, each complete. Both
/// must read the same `commit.gpgsign`; when it is true, both must sign.
fn require_signing_with(
    repo_root: &Path,
    operator: &[(OsString, OsString)],
    daemon: &[(OsString, OsString)],
) -> Result<()> {
    let wanted = signing::signing_enabled_in(repo_root, operator)
        .context("cannot read commit.gpgsign in your environment")?;
    let seen = signing::signing_enabled_in(repo_root, daemon)
        .context("cannot read commit.gpgsign in the daemon's environment")?;
    if wanted != seen {
        bail!(
            "commit.gpgsign is {wanted} in your environment but {seen} in the one the daemon runs \
             with: it comes from configuration the daemon's environment cannot see, such as \
             GIT_CONFIG_COUNT/GIT_CONFIG_KEY_0/GIT_CONFIG_VALUE_0 or git -c \
             (GIT_CONFIG_PARAMETERS). Set it in a config file instead (git config --global \
             commit.gpgsign {wanted}), then run loom run again."
        );
    }
    if !wanted {
        return Ok(());
    }
    signing::probe_in(repo_root, operator)?;
    signing::probe_in(repo_root, daemon)?;
    let format = signing::signing_format_in(repo_root, daemon)?;
    if cache_caveat_applies(format.as_deref()) {
        eprintln!("{CACHE_CAVEAT}");
    }
    Ok(())
}

/// gpg-agent's passphrase cache expires; an ssh-agent key or an X.509 signer
/// has no such limit loom can name.
fn cache_caveat_applies(format: Option<&str>) -> bool {
    matches!(format, None | Some("openpgp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::signing::tests::{fake_signer, git_in, isolated_env};
    use tempfile::TempDir;

    fn repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        git_in(temp.path(), &["init", "-b", "main"]);
        git_in(temp.path(), &["config", "user.email", "t@t.com"]);
        git_in(temp.path(), &["config", "user.name", "t"]);
        temp
    }

    /// `isolated_env` with `commit.gpgsign=true` set through the
    /// `GIT_CONFIG_COUNT` variables only.
    fn operator_env(dir: &Path) -> Vec<(OsString, OsString)> {
        let mut env = isolated_env(dir);
        for (name, value) in [
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "commit.gpgsign"),
            ("GIT_CONFIG_VALUE_0", "true"),
        ] {
            env.push((name.into(), value.into()));
        }
        env
    }

    #[test]
    fn require_signing_passes_when_signing_is_off() {
        let temp = repo();
        git_in(temp.path(), &["config", "commit.gpgsign", "false"]);
        let env = isolated_env(temp.path());

        require_signing_with(temp.path(), &env, &env).unwrap();
    }

    #[test]
    fn require_signing_names_the_signer_failure() {
        let temp = repo();
        fake_signer(temp.path(), true);
        let env = isolated_env(temp.path());

        let error = require_signing_with(temp.path(), &env, &env).unwrap_err();

        let text = format!("{error:#}");
        assert!(text.contains("a test signature failed"), "{text}");
        assert!(text.contains("fake signer refused"), "{text}");
    }

    #[test]
    fn the_daemon_environment_reads_the_same_gpgsign_as_the_operator() {
        let temp = repo();
        let root = temp.path();
        fake_signer(root, false);
        git_in(root, &["config", "--unset", "commit.gpgsign"]);
        let operator = operator_env(root);
        let daemon = isolated_env(root);

        let error = require_signing_with(root, &operator, &daemon).unwrap_err();
        let text = format!("{error:#}");
        assert!(text.contains("commit.gpgsign is true"), "{text}");
        assert!(text.contains("GIT_CONFIG_COUNT"), "{text}");

        require_signing_with(root, &operator, &operator).unwrap();

        fake_signer(root, true);
        let error = require_signing_with(root, &operator, &operator).unwrap_err();
        assert!(
            format!("{error:#}").contains("fake signer refused"),
            "{error:#}"
        );
    }

    #[test]
    fn the_cache_caveat_applies_to_openpgp_and_unset_formats() {
        assert!(cache_caveat_applies(None));
        assert!(cache_caveat_applies(Some("openpgp")));
        assert!(!cache_caveat_applies(Some("ssh")));
        assert!(!cache_caveat_applies(Some("x509")));
    }
}
