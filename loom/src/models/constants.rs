/// The model context window every default ceiling below is derived from.
///
/// Every stage's main agent, and every subagent it spawns, launches on a
/// model with a 1M-token window — this is that number. It is a launch-time
/// property, not something a session can measure: the model string recorded
/// in a transcript is bare (`claude-opus-5`, no window marker), so a heartbeat
/// cannot infer it. `.loom/work/config.toml`'s `[context] model_window_tokens`
/// (see [`crate::fs::work_dir::ContextConfig`]) overrides this per plan, for
/// a plan run against a smaller-window model.
pub const DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS: u32 = 1_000_000;

/// Fraction of the model window a ceiling occupies — the same fraction for a
/// main agent session and for a subagent it spawns.
///
/// A subagent runs the same 1M window as its parent: the two constants below
/// stay separate in NAME, because `[context]` still exposes
/// `ceiling_tokens`/`subagent_ceiling_tokens` as two keys an operator may set
/// apart, but their BUILT-IN defaults come from one fraction of one window —
/// a second fraction would invent a distinction the models themselves do not
/// have.
pub const CONTEXT_CEILING_FRACTION: f32 = 0.80;

/// Derive a ceiling from a model context window and the fraction of it that
/// ceiling occupies.
///
/// Shared by the built-in defaults below and by
/// [`crate::fs::work_dir::ContextConfig`]'s `model_window_tokens` override,
/// so a custom window derives both ceilings through the identical formula
/// the built-in defaults use — changing the window changes both coherently.
pub const fn ceiling_from_window(window_tokens: u32, fraction: f32) -> u32 {
    (window_tokens as f32 * fraction) as u32
}

/// Default context ceiling, in resident tokens, for a stage's agent session.
///
/// Ceilings are absolute token counts, not percentages: the only number a
/// heartbeat can report honestly is how many tokens are resident, and a
/// percentage of an unknown model window is a guess dressed as a measurement.
/// Stages override this via `context_ceiling_tokens`; `.loom/work/config.toml`
/// overrides it plan-wide via `[context] ceiling_tokens`.
///
/// = [`DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS`] (1M) x [`CONTEXT_CEILING_FRACTION`]
/// (80%) = 800,000.
pub const DEFAULT_CONTEXT_CEILING_TOKENS: u32 = ceiling_from_window(
    DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS,
    CONTEXT_CEILING_FRACTION,
);

/// Default context ceiling, in resident tokens, for a subagent.
///
/// Same value as [`DEFAULT_CONTEXT_CEILING_TOKENS`] by default — a subagent
/// runs the same 1M window as its parent, so its built-in ceiling comes from
/// the same [`CONTEXT_CEILING_FRACTION`] of the same window. The name stays
/// distinct because `.loom/work/config.toml`'s `[context] subagent_ceiling_tokens`
/// is still a separate, independently overridable key.
///
/// = [`DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS`] (1M) x [`CONTEXT_CEILING_FRACTION`]
/// (80%) = 800,000.
pub const DEFAULT_SUBAGENT_CEILING_TOKENS: u32 = ceiling_from_window(
    DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS,
    CONTEXT_CEILING_FRACTION,
);

/// Smallest ceiling a stage or config may set, in resident tokens.
///
/// Below this an agent cannot hold its own signal plus a turn of work, so the
/// ceiling would fire before any work could start.
pub const MIN_CONTEXT_CEILING_TOKENS: u32 = 60_000;

/// Multiple of a stage's ceiling at which the DAEMON's backstop would fire,
/// absent the window clamp described below.
///
/// The agent's own hook governs at 100% of the ceiling. The daemon only steps
/// in when that governance was ignored, so it waits until 125% before killing
/// the session out from under the agent.
///
/// At the built-in 800,000 ceiling, `DAEMON_CEILING_MULTIPLIER` alone would
/// put the backstop at 1.25 x 800,000 = 1,000,000 — exactly the model window,
/// making it unreachable in practice: a session dies or compacts before
/// resident tokens ever reach the whole window, so `handle_budget_exceeded`
/// would never fire. [`crate::fs::work_dir::ContextConfig::backstop_tokens`]
/// exists for exactly this reason: it clamps the multiplier's result to
/// `window x `[`DAEMON_BACKSTOP_WINDOW_FRACTION`]` (950,000 at the built-in
/// window), so the backstop stays a reachable trigger with headroom left for
/// the forced handoff it performs. Every daemon-side consumer of this
/// multiplier must go through that method rather than applying it directly.
pub const DAEMON_CEILING_MULTIPLIER: f32 = 1.25;

/// Fraction of the model window the daemon backstop may not exceed, no matter
/// how high `ceiling x `[`DAEMON_CEILING_MULTIPLIER`]` computes.
///
/// See [`crate::fs::work_dir::ContextConfig::backstop_tokens`] for the clamp
/// this bounds and why it exists. At the built-in 1M window this holds the
/// backstop to 950,000 — comfortably inside the window, with room left for
/// the forced handoff the backstop itself triggers.
pub const DAEMON_BACKSTOP_WINDOW_FRACTION: f32 = 0.95;

/// Staleness threshold in seconds for session heartbeats.
/// When a session hasn't sent a heartbeat for this duration,
/// it is considered stale (possibly hung).
pub const STALENESS_THRESHOLD_SECS: u64 = 300; // 5 minutes
