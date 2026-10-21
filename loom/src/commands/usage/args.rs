//! Arguments for `loom usage`.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSelection {
    Claude,
    Codex,
    All,
}

impl ProviderSelection {
    pub(super) fn includes(self, provider: super::provider_types::Provider) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, provider),
                (Self::Claude, super::provider_types::Provider::Claude)
                    | (Self::Codex, super::provider_types::Provider::Codex)
            )
    }
}

/// Arguments for `loom usage`. Lives here rather than in the CLI enum so the
/// command owns its own surface - the same reasoning `SubagentsArgs` documents.
#[derive(Debug, clap::Args)]
pub struct UsageArgs {
    /// Compare a bounded, offline paired-evaluation artifact
    #[arg(long, value_name = "ARTIFACT.json")]
    pub compare: Option<PathBuf>,

    /// How far back to look: a duration (`7d`, `24h`, `30m`), an ISO date (`2026-08-01`) or an RFC3339 instant (`2026-08-01T06:30:00Z`) (default: 7d)
    #[arg(long)]
    pub since: Option<String>,

    /// Inclusive UTC RFC3339 upper bound for event timestamps
    #[arg(long)]
    pub until: Option<String>,

    /// Provider telemetry to normalize (default: claude)
    #[arg(long, value_enum)]
    pub provider: Option<ProviderSelection>,

    /// Explicit Claude projects root; never falls back when supplied
    #[arg(long)]
    pub claude_root: Option<PathBuf>,

    /// Explicit Codex root containing sessions/ and archived_sessions/
    #[arg(long)]
    pub codex_root: Option<PathBuf>,

    /// Explicit directory containing execution receipt protocol files
    #[arg(long)]
    pub receipts_root: Option<PathBuf>,

    /// Single-project Loom state root containing forward receipt stage files
    #[arg(long, value_name = "DIR", conflicts_with = "all")]
    pub forward_receipts_root: Option<PathBuf>,

    /// Project directory to read transcripts for (defaults to this repository
    /// plus each of its `.worktrees/*` subdirectories)
    #[arg(long, conflicts_with = "all")]
    pub project: Option<PathBuf>,

    /// Read every project under ~/.claude/projects/
    #[arg(long)]
    pub all: bool,

    /// Only transcripts belonging to this stage
    #[arg(long)]
    pub stage: Option<String>,

    /// Only transcripts belonging to this plan
    #[arg(long)]
    pub plan: Option<String>,

    /// Which windowing to report the three accountings under (default: 5h)
    #[arg(long, value_enum)]
    pub windows: Option<super::accounting::Windowing>,

    /// Emit machine-readable JSON instead of the table report
    #[arg(long)]
    pub json: bool,
}

impl UsageArgs {
    pub(super) fn since(&self) -> &str {
        self.since.as_deref().unwrap_or("7d")
    }

    pub(super) fn provider(&self) -> ProviderSelection {
        self.provider.unwrap_or(ProviderSelection::Claude)
    }

    pub(super) fn windows(&self) -> super::accounting::Windowing {
        self.windows
            .unwrap_or(super::accounting::Windowing::FiveHour)
    }

    pub(super) fn has_explicit_selection(&self) -> bool {
        self.since.is_some()
            || self.until.is_some()
            || self.provider.is_some()
            || self.claude_root.is_some()
            || self.codex_root.is_some()
            || self.receipts_root.is_some()
            || self.forward_receipts_root.is_some()
            || self.project.is_some()
            || self.all
            || self.stage.is_some()
            || self.plan.is_some()
            || self.windows.is_some()
    }
}
