//! Who is actually on the other end of the daemon socket.
//!
//! Everything here is sourced from the kernel or from `/proc`, never from
//! anything the client said about itself. That is the whole point: a request
//! body can claim any `session_id`, so the claim has to be checked against an
//! identity the caller cannot author.
//!
//! # Why this exists
//!
//! `CompleteStage` is the one RPC a stage agent is *supposed* to make, and it
//! was authenticated with `.work/user.token` — a credential the agent must
//! read to use, and which `sandbox/settings.rs` denies it reading (S-1),
//! because that same token also authorizes every other User-capability RPC. So
//! the only sanctioned way for a worktree stage to complete required reading a
//! file the same generator forbade. Both halves were individually right and
//! mutually exclusive.
//!
//! A secret that must be readable to be usable cannot express "this caller may
//! complete its own stage, and nothing else". Connection identity can: it
//! cannot be read, leaked, or denied, and it is scoped to the process that
//! holds the socket.

use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::models::session::Session;
use crate::orchestrator::terminal::native::{read_pid_entry, NativeBackend};
use crate::parser::frontmatter::parse_from_markdown;
use crate::process::{verify_process_identity, IdentityStatus};

/// How far up the process tree to look for the session's own process.
///
/// The caller is `loom`, invoked by a hook, invoked by the agent, running
/// under the session wrapper — a handful of levels. The bound exists so a
/// `/proc` inconsistency or a pid-reuse cycle cannot spin here.
const MAX_ANCESTRY_DEPTH: usize = 32;

/// The connected client's pid, straight from the kernel.
///
/// `SO_PEERCRED` is captured by the kernel at `connect(2)` time, so it
/// describes the process that actually opened this socket and cannot be
/// spoofed by anything the client sends afterwards.
#[cfg(target_os = "linux")]
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: `stream` owns a live socket fd for the duration of the call, and
    // `cred`/`len` are correctly sized for SO_PEERCRED's `struct ucred`.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&raw mut cred).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    if rc != 0 || cred.pid <= 0 {
        return None;
    }
    Some(cred.pid as u32)
}

/// macOS spells the same thing `LOCAL_PEERPID` on `SOL_LOCAL`.
#[cfg(target_os = "macos")]
pub fn peer_pid(stream: &UnixStream) -> Option<u32> {
    let mut pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // SAFETY: as above; `pid`/`len` are correctly sized for LOCAL_PEERPID.
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast::<libc::c_void>(),
            &raw mut len,
        )
    };
    if rc != 0 || pid <= 0 {
        return None;
    }
    Some(pid as u32)
}

/// Fail closed where the peer cannot be identified: callers fall back to the
/// token path rather than authorizing an unknown peer.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn peer_pid(_stream: &UnixStream) -> Option<u32> {
    None
}

#[cfg(target_os = "linux")]
fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The comm field is parenthesised and may itself contain spaces and
    // parens, so every numeric field is counted from the LAST ") " — the same
    // rule `process::identity` uses. After it: state(0), ppid(1).
    let after_comm = stat.rsplit_once(") ")?.1;
    after_comm.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn parent_pid(pid: u32) -> Option<u32> {
    // No `/proc` to read. `ps` is POSIX and present on macOS; this runs once
    // per completion request, not in any hot path.
    let output = std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

/// Whether `caller` is `ancestor`, or lies somewhere below it.
///
/// Walking up rather than down is deliberate: the caller is several levels
/// below the session's own process (`loom` under a hook under the agent under
/// the wrapper), and only the upward chain is cheap and unambiguous to follow.
fn is_at_or_below(caller: u32, ancestor: u32) -> bool {
    let mut current = caller;
    for _ in 0..MAX_ANCESTRY_DEPTH {
        if current == ancestor {
            return true;
        }
        // pid 1 and pid 0 terminate every chain; stopping here also means a
        // walk that escaped into the init subtree can never match.
        match parent_pid(current) {
            Some(parent) if parent > 1 => current = parent,
            _ => return false,
        }
    }
    false
}

/// Whether the process on the other end of this socket is running inside the
/// session it claims to be.
///
/// Three things must hold, and all three are checked against evidence the
/// caller does not control:
///
/// 1. the kernel gives us the peer's pid;
/// 2. `<work_dir>/sessions/<session_id>.md` records a pid whose start time
///    still matches — so a recycled pid cannot stand in for a dead session
///    (the same `ProcessIdentity` rule the backends use for liveness);
/// 3. the peer is that process, or a descendant of it.
///
/// A caller that names someone else's session fails (3). A caller reusing a
/// dead session's pid fails (2). Nothing here consults the request body beyond
/// the session id it is being asked to prove.
pub fn caller_is_inside_session(work_dir: &Path, session_id: &str, caller_pid: u32) -> bool {
    let relative = std::path::PathBuf::from("sessions").join(format!("{session_id}.md"));
    let Ok(content) =
        crate::fs::safe_read::read_to_string_bounded(work_dir, &relative, 1024 * 1024)
    else {
        return false;
    };
    let Ok(session) = parse_from_markdown::<Session>(&content, "session") else {
        return false;
    };
    // The same PID-file evidence the backends use, read through the same
    // helper — not `session.pid`, which carries no start time and so cannot be
    // told apart from a recycled pid.
    let Some((_, pid_key)) = NativeBackend::window_title_and_pid_key(&session) else {
        return false;
    };
    let Some(identity) = read_pid_entry(work_dir, &pid_key) else {
        return false;
    };
    let session_pid = identity.pid;
    // Fail closed on `Unverifiable`, unlike the liveness helpers. Liveness
    // errs toward "still running" so a session is never reaped on evidence it
    // could not read; authorization must err the other way, because the cost
    // of being wrong is granting a capability rather than delaying a cleanup.
    if verify_process_identity(identity) != IdentityStatus::VerifiedAlive {
        return false;
    }
    is_at_or_below(caller_pid, session_pid)
}

#[cfg(test)]
#[path = "tests/peer_identity.rs"]
mod tests;
