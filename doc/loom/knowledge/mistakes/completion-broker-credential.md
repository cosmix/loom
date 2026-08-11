# Completion Broker Credential

> Topic notes for the mistakes knowledge area.

## A Designed Fallback Existed on the Server, and the Client Could Never Reach It (2026-08-11)

**What happened:** no worktree stage could complete through the trusted PostToolUse broker. Every
attempt died with `Request credential length is outside the allowed range`, the stage stayed
`Executing`, and the merge never ran — the operator had to run `loom stage complete <id>` by hand
from a terminal outside the harness. Observed identically across two machines/projects, so it was
the sanctioned path itself, not an environment quirk.

**The three-layer contradiction, each layer individually defensible:**

1. `commands/stage/control_complete.rs` sent `read_user_token(work_dir).unwrap_or_default()` — an
   EMPTY token when the read fails — with a comment asserting the daemon "falls back to identifying
   the caller by its socket peer credentials". The server really does implement that
   (`Authorization::PendingPeerIdentity` → `peer_identity::caller_is_inside_session`).
2. `daemon/wire.rs::write_request_preface` refuses to frame a credential of length 0, CLIENT-side.
   The request never left the process, so the fallback the comment promised was dead code.
3. The token read fails on this path BY CONSTRUCTION, via two different mechanisms with one
   outcome: in a worktree, `work_dir` is the `.work` SYMLINK and `safe_open_dirfd` opens the root
   `O_NOFOLLOW` (ELOOP); under a sandboxed hook, the deny-listed token files read as zero-byte
   character devices and `read_bounded`'s `is_file()` check bails. Either way: `None` → `""` →
   wire refusal.

**Red herring that cost a peer session real time:** "the token is a 64-byte file — is 64 the
problem?" No length between 1 and 256 is ever the problem; the only unframeable length is 0. When
this error appears, the credential is EMPTY, which means the token READ failed — investigate the
read path (symlinked root, sandbox masking), not the token's contents.

**Fix (`completion_credential`, same file):** when the token is unreadable or empty, send the fixed
placeholder `peer-identity` instead of `""`. Any non-matching credential routes the daemon into
`PendingPeerIdentity`, which authorizes exactly one thing — a caller completing the session it is
actually running inside, proven by kernel `SO_PEERCRED` plus PID-ancestry — so the placeholder
grants nothing by itself. Tests pin the symlinked-root reproduction and the wire-level refusal of
`""`.

**Prevention:**

- A designed fallback must be REACHABLE end-to-end. When a client comment says "the server falls
  back", trace the exact bytes the client emits in that case through every framing/validation layer
  between the two. A fallback only exercised in server-side unit tests has never actually run.
- "Absent" must be representable on the wire. If a protocol layer rejects the empty encoding of
  a value, every caller that can legitimately lack that value needs an explicit non-empty
  encoding for absence — `unwrap_or_default()` on a credential is exactly the bug shape to grep for.
- `fs/safe_read` refuses BOTH a symlinked root (`O_NOFOLLOW` on `safe_open_dirfd`) and non-regular
  files. Any caller handing it a worktree's `.work` path, or a sandbox-masked path, gets `Err` even
  though the underlying file is fine. That is deliberate hardening — design callers so the failure
  is survivable, as the broker now does.

## Two Files Are Named `control_complete.rs`

`commands/stage/control_complete.rs` (broker CLIENT: credential selection, socket send) and
`daemon/server/control_complete.rs` (daemon-side transition HANDLER) are unrelated files sharing a
basename, each wired via `#[path]` from a different parent. `cargo test control_complete` runs both
test sets, and a bare-filename grep finds both — qualify with the directory before editing.
