# Completion Broker Credential

> The completion broker unreachable server-side fallback, duplicate file naming, and a sandboxed completion that exits 0 without completing.

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

## A Sandboxed `stage complete` Exits 0 Without Completing Anything (2026-08-26)

**What happened:** a stage reported that it had completed successfully when it had not. In a
sandboxed worktree, `loom stage complete` never completes the stage: `run_verification_phase`
verifies, prints the `LOOM_CONTROL_VERIFICATION_PASSED` marker, and returns `Ok(())`. The real
transition is applied out of band by `hooks/loom-control-complete.sh`, which reads that marker
back out of the tool result and calls the daemon broker.

**Why it was invisible:** the agent's only evidence was exit 0 plus a line containing "PASSED",
both emitted *before* anything was completed and both unchanged whether or not the transition
ever happened. The bridge had two bare `exit 0` skips — the command reported an error, or the
marker was absent from the captured output — that called no broker and said nothing. The marker
test is an exact whole-line match (`split("\n") | index($marker)`), so truncated, wrapped or
CR-terminated stdout makes it vanish silently. Two more routes (`DaemonManaged`, `SpawnResolver`)
also return `Ok(())` after printing informational text.

**And the backstop was gone.** `commit-guard.sh` is supposed to catch a session ending with the
stage still Executing; `warn_with_reason` always exits 0, deliberately, because Claude Code fires
Stop hooks during Task-tool waits. `CLAUDE.md.template` hard stop 3 nevertheless promised "the
stop hook blocks exit otherwise" — in BOTH the rule and its verbatim recap. Doctrine asserting a
guarantee the code stopped providing is worse than no guarantee: agents rely on it.

**Prevention:** when success is reported by one component and applied by another, the reporting
side must say it has not applied anything, and every silent skip on the applying side must
explain itself. Check any `|| exit 0` in a bridge hook: it is indistinguishable from success.
When a rule promises enforcement, grep the enforcing code for the exit path that delivers it.

**Fix:** the sandboxed path now names the daemon confirmation to wait for; both bridge skips emit
`hookSpecificOutput.additionalContext` (still exiting 0 — it is PostToolUse); the two daemon
routes say they did not complete the stage; both template copies say the stop hook only warns.

**The marker line is frozen and pinned.** `complete.rs` prints it and the bridge matches it as a
whole line, so rewording it disables completion everywhere with nothing failing loudly. A test
pins the exact text. Verify any change to it by deriving both sides independently and comparing
bytes — not by eye.

## The Truncation Risk Was Real: a 2KB Preview Ate the Marker (2026-08-27)

**What happened:** the truncation hazard noted above stopped being hypothetical. A stage's
`loom stage complete` ran green, printed the marker, and left the stage `Executing`. Confirmed
from the session transcript: the tool result was **persisted**, and the marker was absent from
what the bridge could read. An earlier attempt in the same stage produced 20.6KB — under the
threshold — and completed normally. Purely a size threshold.

**Why:** past an internal size limit Claude Code does not put the output in the tool result at
all. It writes the output to a file and substitutes a wrapper:

```text
<persisted-output>
Output too large (43.5KB). Full output saved to: /home/<user>/.claude/projects/<slug>/<session>/tool-results/<id>.txt

Preview (first 2KB):
<first 2KB of the real output>
</persisted-output>
```

The marker sat at line 1362 of 1364, so `HAS_MARKER` — an exact whole-line match over the tool
result — could never see it. The flood that inflated the output came from loom's own grep calls
(`mistakes/schema-reuse-and-silent-skips.md`).

**Fix:** when the inline check fails, the bridge resolves the `Full output saved to:` path and
looks for the marker there. **The wrapper text is agent-influenceable** — `loom stage complete`
echoes acceptance-criteria output and duplicate-detection prints matched source lines back out —
so a stage could otherwise print a forged wrapper naming a file it wrote itself. The path is
accepted only when it is absolute, under `$HOME/.claude/projects`, contains a `/tool-results/`
segment, contains **no `..` path segment**, exists as a regular file, and is not a symlink.

**What makes that sound, and the trap inside it.** The guarantee is that `~/.claude/projects/` is
write-denied to the sandboxed stage agent, so only the harness can author a file there. The first
version of the check tested containment with a glob over the raw string, which a traversal
satisfies while resolving elsewhere:

```text
candidate:   $HOME/.claude/projects/../../../../tmp/claude-1000/tool-results/fake.txt
resolves to: /tmp/claude-1000/tool-results/fake.txt        # agent-WRITABLE
```

`/tmp/claude*` and `$TMPDIR` are in the agent's sandbox write allowlist, so that was a working
forgery of a completion marker. See `mistakes/untrusted-value-boundaries.md` § "A Glob Is Not a
Containment Check".

**Prevention:** fixing the marker's *carrier* does not fix the marker's *volume*. Keep the
completion command's output small — a verbose verification stage will find this threshold again.
