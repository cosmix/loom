use std::path::Path;

const NO_DATA: &str = "No fresh quota data: loom polls quota while the daemon (loom run) or the planning server (loom plan) is running.";

pub(super) fn render_model_budget(quota: &crate::quota::QuotaSnapshot, now: i64) -> String {
    crate::orchestrator::signals::format_model_budget_section(quota, now)
        .unwrap_or_else(|| NO_DATA.to_string())
}

pub(super) fn print_model_budget(work_dir: Option<&Path>) -> anyhow::Result<()> {
    let rendered = work_dir.map_or_else(
        || NO_DATA.to_string(),
        |dir| {
            render_model_budget(
                &crate::quota::read_snapshot(dir),
                chrono::Utc::now().timestamp(),
            )
        },
    );
    println!("{rendered}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind};

    fn fresh_claude(now: i64) -> ProviderQuota {
        ProviderQuota {
            observed_at: now,
            windows: vec![QuotaWindow {
                kind: WindowKind::FiveHour,
                used_percent: 50.0,
                resets_at: None,
            }],
            plan: None,
            error: None,
        }
    }

    #[test]
    fn renders_empty_and_fresh_snapshots() {
        assert_eq!(render_model_budget(&QuotaSnapshot::default(), 100), NO_DATA);
        let snapshot = QuotaSnapshot {
            claude: Some(fresh_claude(100)),
            codex: None,
        };
        assert!(render_model_budget(&snapshot, 100).starts_with("## Model Budget"));
    }

    #[test]
    fn renders_fresh_cache_read_from_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let now = chrono::Utc::now().timestamp();
        crate::quota::cache::write_provider(dir.path(), "claude", &fresh_claude(now)).unwrap();
        assert!(
            render_model_budget(&crate::quota::read_snapshot(dir.path()), now)
                .starts_with("## Model Budget")
        );
    }
}
