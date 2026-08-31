//! `ContextConfig`: the typed `[context]` section of `.work/config.toml`.
//!
//! Split out of `work_dir.rs` so the read/write/section plumbing shared by
//! every `.work/config.toml` section (`read_section`, `write_section`,
//! `merge_section`, and the `read_*_config`/`write_*_config` wrappers) stays
//! in one file while the context-ceiling-specific type, its deserialization,
//! and its derived values (`ceiling_for`, `backstop_tokens`) live in another.
//! Re-exported as [`crate::fs::work_dir::ContextConfig`], so no caller's
//! import path changes.

use serde::{Deserialize, Serialize};

use crate::models::constants::{
    ceiling_from_window, CONTEXT_CEILING_FRACTION, DAEMON_BACKSTOP_WINDOW_FRACTION,
    DAEMON_CEILING_MULTIPLIER, DEFAULT_CONTEXT_CEILING_TOKENS, DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS,
    DEFAULT_SUBAGENT_CEILING_TOKENS,
};

/// Persisted `[context]` section of `.work/config.toml`: the plan-wide context
/// ceilings, in absolute resident tokens.
///
/// Sits between a stage's own `context_ceiling_tokens` and the built-in
/// defaults, so an operator can raise or lower every stage's ceiling in one
/// place without editing the plan. Each field resolves to a usable number even
/// when the section is half-written or absent — see `ContextConfigRaw` for
/// how. That type is private, so this is a plain code span: a bracketed link
/// would resolve only under `--document-private-items`, which the docs gate
/// does not pass.
///
/// `model_window_tokens`, when set, replaces the built-in 1M-token window as
/// what `ceiling_tokens` and `subagent_ceiling_tokens` derive from — via the
/// same [`CONTEXT_CEILING_FRACTION`] for both — for whichever of those two
/// fields the section leaves unset. An explicit
/// `ceiling_tokens`/`subagent_ceiling_tokens` in the TOML always wins over the
/// derivation, on either window. This is what lets a plan running against a
/// smaller-window model set one number instead of hand-computing two: alone,
/// `model_window_tokens = 200000` yields `ceiling_tokens = 160000` and
/// `subagent_ceiling_tokens = 160000`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(from = "ContextConfigRaw")]
pub struct ContextConfig {
    /// Ceiling for a stage's own agent session.
    pub ceiling_tokens: u32,
    /// Ceiling for a subagent spawned by that session.
    pub subagent_ceiling_tokens: u32,
    /// The model context window `ceiling_tokens` and `subagent_ceiling_tokens`
    /// derive from when the TOML leaves either unset. `None` uses the built-in
    /// 1M-token window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_window_tokens: Option<u32>,
}

/// Deserialization shadow for [`ContextConfig`], every field optional.
///
/// A plain per-field `#[serde(default = "...")]` cannot make `ceiling_tokens`
/// derive from a sibling field's value (`model_window_tokens`) — field
/// defaults are evaluated independently, with no view of the rest of the
/// struct. Deserializing into this shadow first, then converting via
/// [`ContextConfig`]'s `From` impl, is what lets that conversion tell "the
/// TOML set this key" apart from "the TOML left this to derive".
#[derive(Debug, Default, Deserialize)]
struct ContextConfigRaw {
    ceiling_tokens: Option<u32>,
    subagent_ceiling_tokens: Option<u32>,
    model_window_tokens: Option<u32>,
}

impl From<ContextConfigRaw> for ContextConfig {
    fn from(raw: ContextConfigRaw) -> Self {
        // One formula ("derive a ceiling from a window", `ceiling_from_window`)
        // and one fraction for both fields — a subagent runs the same window as
        // its parent, so main and subagent get the same default for a set
        // window exactly as they do for the built-in one. When no window is
        // set, this falls back to the pre-derived built-in constants rather
        // than recomputing them, so there is exactly one place either number
        // is spelled out.
        let default_for = |window: Option<u32>, built_in: u32| match window {
            Some(window) => ceiling_from_window(window, CONTEXT_CEILING_FRACTION),
            None => built_in,
        };
        Self {
            ceiling_tokens: raw.ceiling_tokens.unwrap_or_else(|| {
                default_for(raw.model_window_tokens, DEFAULT_CONTEXT_CEILING_TOKENS)
            }),
            subagent_ceiling_tokens: raw.subagent_ceiling_tokens.unwrap_or_else(|| {
                default_for(raw.model_window_tokens, DEFAULT_SUBAGENT_CEILING_TOKENS)
            }),
            model_window_tokens: raw.model_window_tokens,
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        ContextConfigRaw::default().into()
    }
}

impl ContextConfig {
    /// The ceiling governing a stage's agent session, for a caller that already
    /// holds this config. The monitor reads `[context]` once per run rather than
    /// once per session per tick, so it resolves against the config in hand.
    ///
    /// This is where loom's ceiling order is defined;
    /// [`crate::fs::work_dir::resolve_context_ceiling_tokens`] is the same
    /// order for a caller that has only a work dir.
    pub fn ceiling_for(&self, stage_ceiling: Option<u32>) -> u32 {
        stage_ceiling.unwrap_or(self.ceiling_tokens)
    }

    /// The daemon's backstop for a resolved ceiling, in absolute resident
    /// tokens — the point past which the daemon kills a session that ignored
    /// its own hook's 100% warning.
    ///
    /// `ceiling x `[`DAEMON_CEILING_MULTIPLIER`] alone stops being a REACHABLE
    /// trigger once the ceiling gets close enough to the model window: at the
    /// built-in 800,000 ceiling it lands at exactly 1,000,000, the whole
    /// window, which a session dies or compacts before ever reaching. Clamping
    /// to `window x `[`DAEMON_BACKSTOP_WINDOW_FRACTION`] (950,000 at the
    /// built-in window) keeps it a trigger the daemon can actually observe,
    /// with headroom left for the forced handoff it performs.
    ///
    /// **Every daemon-side comparison against `DAEMON_CEILING_MULTIPLIER` must
    /// call this method instead of applying the multiplier directly** — see
    /// `orchestrator/monitor/detection.rs::detect_backstop_crossing`, whose
    /// `ceiling as f32 * DAEMON_CEILING_MULTIPLIER` this method is meant to
    /// replace. That call site is owned outside this module; wiring the call
    /// through is a separate, explicit step, not implied by this method's
    /// existence.
    pub fn backstop_tokens(&self, ceiling: u32) -> u32 {
        let window = self
            .model_window_tokens
            .unwrap_or(DEFAULT_MODEL_CONTEXT_WINDOW_TOKENS);
        let multiplier_backstop = ceiling_from_window(ceiling, DAEMON_CEILING_MULTIPLIER);
        let window_backstop = ceiling_from_window(window, DAEMON_BACKSTOP_WINDOW_FRACTION);
        multiplier_backstop.min(window_backstop)
    }
}

#[cfg(test)]
mod tests;
