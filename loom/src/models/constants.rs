/// Default context ceiling, in resident tokens, for a stage's agent session.
///
/// Ceilings are absolute token counts, not percentages: the only number a
/// heartbeat can report honestly is how many tokens are resident, and a
/// percentage of an unknown model window is a guess dressed as a measurement.
/// Stages override this via `context_ceiling_tokens`; `.work/config.toml`
/// overrides it plan-wide via `[context] ceiling_tokens`.
pub const DEFAULT_CONTEXT_CEILING_TOKENS: u32 = 150_000;

/// Default context ceiling, in resident tokens, for a subagent.
///
/// Lower than the session ceiling: a subagent's output has to fit back into
/// the orchestrator that spawned it.
pub const DEFAULT_SUBAGENT_CEILING_TOKENS: u32 = 120_000;

/// Smallest ceiling a stage or config may set, in resident tokens.
///
/// Below this an agent cannot hold its own signal plus the files it must read,
/// so the ceiling would fire before any work could start.
pub const MIN_CONTEXT_CEILING_TOKENS: u32 = 60_000;

/// Multiple of a stage's ceiling at which the DAEMON forces a handoff.
///
/// The agent's own hook governs at 100% of the ceiling. The daemon only steps
/// in when that governance was ignored, so it waits until 125% before killing
/// the session out from under the agent.
pub const DAEMON_CEILING_MULTIPLIER: f32 = 1.25;

/// Staleness threshold in seconds for session heartbeats.
/// When a session hasn't sent a heartbeat for this duration,
/// it is considered stale (possibly hung).
pub const STALENESS_THRESHOLD_SECS: u64 = 300; // 5 minutes
