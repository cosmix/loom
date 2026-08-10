//! One-time operator proof verification for privileged stage completion.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use super::admin_hmac::{constant_time_eq, hmac_sha256};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// Environment variable used to pass a proof from a trusted host broker.
///
/// The value is deliberately not accepted as a command-line argument, so it is
/// absent from process listings and shell history. Callers must remove it from
/// the environment before spawning any stage runtime.
pub const ADMIN_PROOF_ENV: &str = "LOOM_ADMIN_PROOF";

/// Environment variable used only by the trusted proof-minting command.
pub const ADMIN_SECRET_ENV: &str = "LOOM_ADMIN_TOKEN";

const PROOF_VERSION: &str = "v1";
const COMPLETION_ACTION: &str = "stage.complete";
const DAEMON_STOP_ACTION: &str = "daemon.stop";
const DAEMON_TARGET: &str = "daemon";
const REPLAY_DIR: &str = "admin-proof-replays";

/// The privileged operation authorized by an admin proof.
#[derive(Debug, Clone, Copy)]
pub struct AdminProofRequest<'a> {
    stage_id: &'a str,
    action: &'a str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
}

impl<'a> AdminProofRequest<'a> {
    /// Build the request for `loom stage complete` and its exact flag set.
    pub fn completion(
        stage_id: &'a str,
        no_verify: bool,
        force_unsafe: bool,
        assume_merged: bool,
    ) -> Self {
        Self {
            stage_id,
            action: COMPLETION_ACTION,
            no_verify,
            force_unsafe,
            assume_merged,
        }
    }

    /// Build the request for stopping the daemon serving this work directory.
    ///
    /// The per-work-directory secret binds the proof to the project. The fixed
    /// daemon target and action keep it distinct from every stage capability.
    pub fn daemon_stop() -> AdminProofRequest<'static> {
        AdminProofRequest {
            stage_id: DAEMON_TARGET,
            action: DAEMON_STOP_ACTION,
            no_verify: false,
            force_unsafe: false,
            assume_merged: false,
        }
    }

    #[cfg(test)]
    fn with_action(mut self, action: &'a str) -> Self {
        self.action = action;
        self
    }

    fn canonical_message(&self, nonce: &str) -> String {
        format!(
            "{PROOF_VERSION}\nstage={}:{}\naction={}:{}\nflags=no_verify:{},force_unsafe:{},assume_merged:{}\nnonce={}:{}",
            self.stage_id.len(),
            self.stage_id,
            self.action.len(),
            self.action,
            u8::from(self.no_verify),
            u8::from(self.force_unsafe),
            u8::from(self.assume_merged),
            nonce.len(),
            nonce
        )
    }
}

/// Take the one-time proof from the process environment.
///
/// Removal happens immediately so acceptance commands and other child
/// processes cannot inherit the capability.
pub fn take_admin_proof_from_env() -> Result<String> {
    let value = std::env::var_os(ADMIN_PROOF_ENV);
    // Single-threaded CLI dispatch invariant: this runs before command code can
    // start worker threads. Rust 2024 makes environment mutation unsafe because
    // it cannot be synchronized with arbitrary foreign environment readers; if
    // dispatch ever becomes concurrent, replace this contract with an inherited
    // protected file descriptor rather than moving this removal later.
    std::env::remove_var(ADMIN_PROOF_ENV);

    let value = value.ok_or_else(|| {
        anyhow::anyhow!(
            "privileged completion requires a one-time operator proof in {ADMIN_PROOF_ENV}. \
             Mint one with `loom stage admin-proof <stage-id>` using the same privileged flags \
             and {ADMIN_SECRET_ENV} set in that command's environment"
        )
    })?;
    value
        .into_string()
        .map_err(|_| anyhow::anyhow!("{ADMIN_PROOF_ENV} must contain a valid UTF-8 operator proof"))
}

/// Remove privileged completion material before a stage runtime is spawned.
///
/// The orchestrator calls this on its launch-owner thread before invoking a
/// backend. These variables are never read by daemon worker threads. If the
/// project moves to Rust 2024, that ownership invariant must be preserved when
/// wrapping these environment mutations in `unsafe`.
pub(crate) fn strip_privileged_env_for_runtime() {
    std::env::remove_var(ADMIN_PROOF_ENV);
    std::env::remove_var(ADMIN_SECRET_ENV);
}

/// Verify and consume a proof for one exact privileged request.
///
/// Proof format: `v1:<nonce>:<hex HMAC-SHA256>`. The HMAC input includes the
/// stage ID, action, and every privileged flag. A successfully verified proof
/// is consumed with an atomic `create_new` marker, so concurrent or later replay
/// fails before the requested action runs.
pub fn verify_and_consume_admin_proof(
    work_dir: &Path,
    request: AdminProofRequest<'_>,
    proof: Option<&str>,
) -> Result<()> {
    let proof = proof.ok_or_else(|| {
        anyhow::anyhow!(
            "privileged completion requires a one-time operator proof in {ADMIN_PROOF_ENV}. \
             Mint one with `loom stage admin-proof <stage-id>` using the same privileged flags"
        )
    })?;
    let (nonce, supplied_mac) = parse_proof(proof)?;
    let resolved_work_dir = resolve_work_dir(work_dir)?;
    let secret = read_admin_secret(&resolved_work_dir)?;
    let message = request.canonical_message(nonce);
    let expected_mac = hmac_sha256(secret.as_bytes(), message.as_bytes());

    if !constant_time_eq(&expected_mac, &supplied_mac) {
        bail!("operator proof is invalid for this stage, action, or privileged flag set");
    }

    consume_replay_marker(&resolved_work_dir, &message, proof)
}

fn parse_proof(proof: &str) -> Result<(&str, Vec<u8>)> {
    let mut parts = proof.split(':');
    let version = parts.next();
    let nonce = parts.next();
    let mac = parts.next();
    if version != Some(PROOF_VERSION) || parts.next().is_some() {
        bail!("operator proof has an invalid format");
    }
    let nonce = nonce.ok_or_else(|| anyhow::anyhow!("operator proof is missing its nonce"))?;
    if !(16..=128).contains(&nonce.len())
        || !nonce
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("operator proof nonce must be 16-128 URL-safe ASCII characters");
    }
    let mac = mac.ok_or_else(|| anyhow::anyhow!("operator proof is missing its authenticator"))?;
    let decoded = hex::decode(mac).context("operator proof authenticator is not valid hex")?;
    Ok((nonce, decoded))
}

fn read_admin_secret(work_dir: &Path) -> Result<String> {
    const MAX_ADMIN_TOKEN_BYTES: usize = 4096;
    let secret = crate::fs::safe_read::read_to_string_bounded(
        work_dir,
        Path::new("admin.token"),
        MAX_ADMIN_TOKEN_BYTES,
    )
    .with_context(|| "admin proof verifier is unavailable; start the Loom daemon")?;
    let secret = secret.trim().to_string();
    if secret.is_empty() {
        bail!("admin proof verifier is unavailable; daemon credential is empty");
    }
    Ok(secret)
}

fn resolve_work_dir(work_dir: &Path) -> Result<PathBuf> {
    work_dir.canonicalize().with_context(|| {
        format!(
            "failed to resolve work directory for operator proof verification: {}",
            work_dir.display()
        )
    })
}

fn consume_replay_marker(work_dir: &Path, message: &str, proof: &str) -> Result<()> {
    let replay_dir = Path::new(REPLAY_DIR);
    crate::fs::safe_fs::safe_create_dir_all(work_dir, replay_dir, 0o700)
        .context("failed to create proof replay directory")?;
    enforce_private_directory(work_dir, replay_dir)?;

    let mut digest = Sha256::new();
    digest.update(message.as_bytes());
    digest.update([0]);
    digest.update(proof.as_bytes());
    let marker_path = replay_dir.join(hex::encode(digest.finalize()));

    match crate::fs::safe_fs::safe_create_new(work_dir, &marker_path, b"v1\n") {
        Ok(()) => Ok(()),
        Err(error) if is_already_exists(&error) => {
            bail!("operator proof has already been used")
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to consume operator proof at {}",
                marker_path.display()
            )
        }),
    }
}

fn enforce_private_directory(work_dir: &Path, relative: &Path) -> Result<()> {
    let directory = crate::fs::safe_fs::safe_open_dirfd(&work_dir.join(relative))?;
    // SAFETY: `directory` is a live descriptor opened with O_NOFOLLOW, and the
    // mode is a fixed owner-only permission mask.
    if unsafe { libc::fchmod(directory.as_raw_fd(), 0o700) } < 0 {
        return Err(std::io::Error::last_os_error())
            .context("failed to set proof replay directory permissions");
    }
    Ok(())
}

fn is_already_exists(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| io_error.kind() == std::io::ErrorKind::AlreadyExists)
    })
}

pub(crate) fn mint_admin_proof(
    secret: &str,
    request: AdminProofRequest<'_>,
    nonce: &str,
) -> String {
    let mac = hmac_sha256(
        secret.as_bytes(),
        request.canonical_message(nonce).as_bytes(),
    );
    format!("{PROOF_VERSION}:{nonce}:{}", hex::encode(mac))
}

/// Mint a completion proof from an operator-supplied environment secret.
///
/// This command never reads `admin.token`: an untrusted caller that can invoke
/// Loom but cannot read the token therefore cannot turn Loom into a credential
/// oracle. A wrong supplied secret simply produces a proof that verification
/// rejects.
pub fn mint_completion_proof_from_env(
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
) -> Result<String> {
    let secret = take_admin_secret_from_env()?;
    let request = AdminProofRequest::completion(stage_id, no_verify, force_unsafe, assume_merged);
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    Ok(mint_admin_proof(&secret, request, &nonce))
}

/// Environment variable every loom-spawned session's wrapper exports, for all
/// session kinds. Its presence is what [`refuse_operator_inside_a_session`]
/// keys on.
const SESSION_ID_ENV: &str = "LOOM_SESSION_ID";

/// Refuse operator self-authorization from inside any loom-spawned session.
///
/// # This is a guard rail, not the boundary
///
/// The boundary is the sandbox denying `.work/admin.token`; an agent could
/// clear this variable, so nothing here withstands a caller that is actually
/// trying. It exists because self-authorization is now the DEFAULT rather than
/// something a human opts into, which is right for the operator and would
/// otherwise hand an agent a one-word path to the same thing. The easy path
/// fails closed and says what to do instead.
fn refuse_operator_inside_a_session() -> Result<()> {
    if let Some(session) = std::env::var_os(SESSION_ID_ENV) {
        bail!(
            "privileged operations are authorized by the human running loom, not by the \
             agent executing a stage — and this is loom session {}. If the stage genuinely \
             cannot pass its acceptance criteria, use `loom stage dispute-criteria`",
            session.to_string_lossy()
        );
    }
    Ok(())
}

/// Obtain the proof for a privileged operation, however this caller is
/// entitled to obtain one.
///
/// # An operator is never asked to mint anything
///
/// There is exactly one way to hold this capability — being able to read
/// `.work/admin.token` — and an operator shell already can. Making them carry
/// an HMAC from one command to another added no security whatsoever; the
/// person doing it had the credential the whole time. So the ceremony is gone:
/// a privileged command mints its own proof and proceeds.
///
/// `LOOM_ADMIN_PROOF` still wins when present, because a trusted broker minting
/// out of band is a real case (`loom stage admin-proof`) and its proof is
/// bound more narrowly than what this function would produce.
///
/// # What still stops an agent
///
/// The same two things as before, both checked below and neither of them a
/// flag the caller chooses: `sandbox/settings.rs` denies `.work/admin.token`
/// to every stage agent (S-1), and Claude Code's sandbox binds the whole
/// process tree, so a `loom` an agent spawns cannot read it either. On top of
/// that, [`refuse_operator_inside_a_session`] closes the easy path before the
/// read is even attempted.
pub fn authorize(work_dir: &Path, request: AdminProofRequest<'_>) -> Result<Option<String>> {
    if std::env::var_os(ADMIN_PROOF_ENV).is_some() {
        return take_admin_proof_from_env().map(Some);
    }
    refuse_operator_inside_a_session()?;
    let Ok(resolved) = resolve_work_dir(work_dir) else {
        return Ok(None);
    };
    if !admin_credential_exists(&resolved) {
        return Ok(None);
    }
    let secret = read_admin_secret(&resolved).context(
        "this operation is the operator's to authorize, but the daemon credential \
         could not be read. Inside a sandboxed stage worktree that read is denied by design",
    )?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    Ok(Some(mint_admin_proof(&secret, request, &nonce)))
}

/// Whether a daemon credential exists at all.
///
/// `admin.token` is written when the daemon starts and removed when it is not
/// running, so its absence means there is no verifier and no proof is
/// obtainable — by anyone, operator included. Callers treat that as "no proof
/// required" rather than as a refusal; see
/// `complete::authorize_privileged_completion` for why that is safe.
pub fn admin_credential_exists(work_dir: &Path) -> bool {
    work_dir.join("admin.token").exists()
}

/// Mint a one-time proof authorizing daemon shutdown for this work directory.
pub fn mint_daemon_stop_proof_from_env() -> Result<String> {
    let secret = take_admin_secret_from_env()?;
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    Ok(mint_admin_proof(
        &secret,
        AdminProofRequest::daemon_stop(),
        &nonce,
    ))
}

fn take_admin_secret_from_env() -> Result<String> {
    let value = std::env::var_os(ADMIN_SECRET_ENV);
    // Same single-threaded dispatch invariant as `take_admin_proof_from_env`.
    std::env::remove_var(ADMIN_SECRET_ENV);
    let value = value.ok_or_else(|| {
        anyhow::anyhow!("proof minting requires the daemon admin token in {ADMIN_SECRET_ENV}")
    })?;
    let secret = value.into_string().map_err(|_| {
        anyhow::anyhow!("{ADMIN_SECRET_ENV} must contain a valid UTF-8 daemon token")
    })?;
    if secret.trim().is_empty() {
        bail!("{ADMIN_SECRET_ENV} must not be empty");
    }
    Ok(secret.trim().to_string())
}

#[cfg(test)]
#[path = "tests/admin_proof.rs"]
mod tests;
