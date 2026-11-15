use std::path::Path;

use crate::orchestrator::signals::format_model_budget_section;

const NO_DATA: &str = "No fresh quota data: loom polls quota while the daemon (loom run) or the planning server (loom plan) is running.";

fn render_model_budget(work_dir: Option<&Path>) -> String {
    work_dir
        .and_then(|dir| {
            format_model_budget_section(
                &crate::quota::read_snapshot(dir),
                chrono::Utc::now().timestamp(),
            )
        })
        .unwrap_or_else(|| NO_DATA.to_string())
}

pub(super) fn print_model_budget(work_dir: Option<&Path>) {
    println!("{}", render_model_budget(work_dir));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{ProviderQuota, QuotaWindow, WindowKind};

    fn fresh_claude(now: i64, windows: Vec<QuotaWindow>) -> ProviderQuota {
        ProviderQuota {
            observed_at: now,
            windows,
            plan: None,
            error: None,
        }
    }

    fn five_hour(percent: f64) -> Vec<QuotaWindow> {
        vec![QuotaWindow {
            kind: WindowKind::FiveHour,
            used_percent: percent,
            resets_at: None,
        }]
    }

    #[test]
    fn no_work_dir_or_empty_cache_prints_the_no_data_line() {
        assert_eq!(render_model_budget(None), NO_DATA);
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(render_model_budget(Some(dir.path())), NO_DATA);
    }

    #[test]
    fn renders_fresh_cache_read_from_work_dir() {
        let dir = tempfile::tempdir().unwrap();
        let now = chrono::Utc::now().timestamp();
        let reading = fresh_claude(now, five_hour(50.0));
        crate::quota::cache::write_provider(dir.path(), "claude", &reading).unwrap();
        assert!(render_model_budget(Some(dir.path())).starts_with("## Model Budget"));
    }

    #[test]
    fn fresh_reading_without_windows_prints_the_no_data_line() {
        let dir = tempfile::tempdir().unwrap();
        let now = chrono::Utc::now().timestamp();
        let reading = fresh_claude(now, Vec::new());
        crate::quota::cache::write_provider(dir.path(), "claude", &reading).unwrap();
        assert_eq!(render_model_budget(Some(dir.path())), NO_DATA);
    }
}
