//! Static shell text for `build_wrapper_script`, split out to keep
//! `wrapper.rs` under its line cap.

use crate::process::AGENT_SESSION_ENV_NAMES;

/// Rebuilds the child environment from a minimal host allowlist rather than
/// inheriting it, so ambient credentials and token-shaped variables never
/// reach a stage session. The forwarded names are generated from
/// [`AGENT_SESSION_ENV_NAMES`], so this list and the host-side environment the
/// auth probe runs under cannot drift; HOME and PATH are written separately.
///
/// TERMINFO/TERMINFO_DIRS travel alongside TERM, together with HOME (already
/// forwarded, which covers `~/.terminfo`) — the standard ncurses resolution
/// inputs. TERM only names the terminal; these say where its capability
/// database lives, and forwarding the name without the database forwards
/// half a contract: any terminal whose terminfo entry is not bundled into
/// the system database (kitty is the observed instance) leaves the stage
/// agent's inherited TERM with nowhere to resolve.
/// USER and LOGNAME name the operator and are not credentials. On macOS the
/// `claude` CLI finds its Keychain login by `$USER`, so a session without it
/// starts "Not logged in".
/// SCCACHE_DIR/SCCACHE_CACHE_SIZE forward an operator's own sccache cache config.
/// RUSTC_WRAPPER is deliberately absent here — `sccache_env` decides it instead,
/// using the session's sandbox verdict this allowlist has no access to.
pub(super) fn env_allowlist() -> String {
    let names = AGENT_SESSION_ENV_NAMES.join(" ");
    format!(
        r#"# Reconstruct the stage environment from a minimal host allowlist. In
# particular, ambient credentials and token-shaped variables are not inherited.
_loom_env=(
    "HOME=${{HOME:-}}"
    "PATH=${{PATH:-/usr/bin:/bin}}"
)
for _loom_name in {names}; do
    _loom_value="${{!_loom_name}}"
    if [ -n "$_loom_value" ]; then
        _loom_env+=("$_loom_name=$_loom_value")
    fi
done
"#
    )
}

/// Records the PID and, best-effort on Linux, the process start-time on line 2
/// so liveness probes can detect PID reuse. `exec` preserves both, so they
/// identify the claude process after it replaces this shell.
pub(super) fn pid_capture(pid_file: &str) -> String {
    format!(
        r#"# Write our PID, then (best-effort, Linux) the process start-time on
# line 2 so liveness probes can detect PID reuse. exec preserves the PID and
# start-time, so these identify the claude process after exec replaces us.
echo $$ > {pid_file}
if [ -r "/proc/$$/stat" ]; then
    # Field 22 of /proc/<pid>/stat is starttime. The comm field (2) is wrapped
    # in parens and may contain spaces, so strip through the last ')' first.
    _loom_stat=$(cat "/proc/$$/stat" 2>/dev/null)
    _loom_after=${{_loom_stat##*) }}
    _loom_start=$(echo "$_loom_after" | awk '{{print $20}}')
    if [ -n "$_loom_start" ]; then
        echo "$_loom_start" >> {pid_file}
    fi
fi
"#
    )
}

/// Rendered above the exec line.
pub(super) const EXEC_COMMENT: &str = r#"# Loom stages record knowledge through `loom memory` / `loom knowledge`; Claude
# Code auto-memory writes to a location invisible to orchestration, so disable
# it at the process boundary rather than by instruction alone.
#
# claude renders its TUI on stdout and prints refusals/fatal errors on
# stderr; teeing only stderr keeps the pane's TTY intact while preserving
# claude's last words after the pane is gone. The tee runs under its own
# `env -i "${_loom_env[@]}"` — the process substitution forks before the
# `exec env -i` below runs, so without it tee would keep the operator's full
# host environment for the whole session, readable from /proc/<tee pid>/environ.
# Replace this process with claude under only the explicit stage contract.
"#;
