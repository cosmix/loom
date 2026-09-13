use serde::Serialize;

use super::provider_types::Provider;
use crate::quota::{HistorySourceState, QuotaHistoryPoint, QuotaHistoryRead};

pub(crate) const QUOTA_HISTORY_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct QuotaHistorySection {
    pub(crate) schema_version: u16,
    pub(crate) providers: Vec<ProviderQuotaHistory>,
}

impl QuotaHistorySection {
    pub(crate) fn new(providers: Vec<ProviderQuotaHistory>) -> Self {
        Self {
            schema_version: QUOTA_HISTORY_SCHEMA_VERSION,
            providers,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ProviderQuotaHistory {
    pub(crate) provider: Provider,
    pub(crate) source: QuotaHistorySourceState,
    pub(crate) diagnostics: QuotaHistoryDiagnosticCounts,
    pub(crate) points: Vec<QuotaHistoryPoint>,
}

impl ProviderQuotaHistory {
    pub(crate) fn unavailable(provider: Provider) -> Self {
        Self {
            provider,
            source: QuotaHistorySourceState::Unavailable,
            diagnostics: QuotaHistoryDiagnosticCounts::default(),
            points: Vec::new(),
        }
    }

    pub(crate) fn from_read(provider: Provider, read: QuotaHistoryRead) -> Self {
        Self {
            provider,
            source: read.diagnostics.source.into(),
            diagnostics: QuotaHistoryDiagnosticCounts {
                malformed_rows: read.diagnostics.malformed_rows,
                unsupported_schema_rows: read.diagnostics.unsupported_schema_rows,
                nonmonotonic_rows: read.diagnostics.nonmonotonic_rows,
            },
            points: read.points,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum QuotaHistorySourceState {
    Unavailable,
    Missing,
    Empty,
    Read,
    Corrupt,
    Unreadable,
    UnknownProvider,
}

impl QuotaHistorySourceState {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Missing => "missing",
            Self::Empty => "empty",
            Self::Read => "read",
            Self::Corrupt => "corrupt",
            Self::Unreadable => "unreadable",
            Self::UnknownProvider => "unknown-provider",
        }
    }
}

impl From<HistorySourceState> for QuotaHistorySourceState {
    fn from(source: HistorySourceState) -> Self {
        match source {
            HistorySourceState::Missing => Self::Missing,
            HistorySourceState::Empty => Self::Empty,
            HistorySourceState::Read => Self::Read,
            HistorySourceState::Corrupt => Self::Corrupt,
            HistorySourceState::Unreadable => Self::Unreadable,
            HistorySourceState::UnknownProvider => Self::UnknownProvider,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct QuotaHistoryDiagnosticCounts {
    pub(crate) malformed_rows: usize,
    pub(crate) unsupported_schema_rows: usize,
    pub(crate) nonmonotonic_rows: usize,
}

#[cfg(test)]
#[path = "quota_history_tests.rs"]
mod tests;
