//! Admin-proof helpers for `loom stage complete` / `loom stage admin-proof`.
//!
//! Split out of `dispatch`, whose top-level match sits at its line ceiling:
//! every new arm has to buy its line back from an existing one, so this pair
//! of privileged-completion helpers moved to their own module instead.

use crate::commands::stage;
use anyhow::Result;

/// The admin proof a `loom stage complete` invocation needs, if any.
///
/// An unprivileged completion needs none. A privileged one authorizes itself
/// through `admin_proof::authorize`, which uses a broker's `LOOM_ADMIN_PROOF`
/// when one is present and otherwise mints from the daemon token the operator
/// can already read. No flag, and nothing for a human to carry between
/// commands.
pub(super) fn resolve_completion_proof(
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> anyhow::Result<Option<String>> {
    if !(no_verify || force_unsafe || assume_merged) {
        return Ok(None);
    }
    // Resolved rather than a hardcoded `.work` literal: only `WorkDir::new`
    // decides whether this project uses the nested or the legacy layout.
    let Ok(work_dir) = crate::fs::work_dir::WorkDir::new(".") else {
        return Ok(None);
    };
    stage::admin_proof::authorize(
        work_dir.root(),
        stage::admin_proof::AdminProofRequest::completion(
            stage_id,
            no_verify,
            force_unsafe,
            assume_merged,
        ),
    )
}

/// `loom stage admin-proof` — mint one capability and print it, nothing else.
///
/// The secret arrives in `LOOM_ADMIN_TOKEN` and is never read from disk here,
/// so a caller that can invoke loom but cannot read `.loom/work/admin.token` gains
/// nothing: a wrong secret simply mints a proof that verification rejects.
/// That is what separates this command from `admin_proof::authorize`, which
/// reads the token and therefore relies on the sandbox to keep an agent out.
pub(super) fn print_minted_proof(
    stage_id: Option<String>,
    daemon_stop: bool,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> Result<()> {
    if daemon_stop {
        println!("{}", stage::admin_proof::mint_daemon_stop_proof_from_env()?);
        return Ok(());
    }
    if !no_verify && !force_unsafe && !assume_merged {
        anyhow::bail!("admin-proof requires at least one privileged completion flag");
    }
    let stage_id = stage_id.expect("clap requires stage_id without --daemon-stop");
    println!(
        "{}",
        stage::complete::mint_completion_proof_from_env(
            &stage_id,
            no_verify,
            force_unsafe,
            assume_merged,
        )?
    );
    Ok(())
}
