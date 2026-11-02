use super::*;

fn collect_merge_summary(stages: &[Stage], work_dir: &WorkDir) -> Result<MergeSummary> {
    // Get merge point for merge report
    let merge_point = if let Some(project_root) = work_dir.project_root() {
        get_merge_point(project_root).unwrap_or_else(|_| "main".to_string())
    } else {
        "main".to_string()
    };

    // Build merge report
    let merge_report = if let Some(project_root) = work_dir.project_root() {
        build_merge_report(stages, &merge_point, project_root)?
    } else {
        crate::status::merge_status::MergeStatusReport::new()
    };

    Ok(build_merge_summary_from_report(&merge_report))
}

/// Collect all status data from the work directory
pub fn collect_status_data(work_dir: &WorkDir) -> Result<StatusData> {
    // Load all stages
    let stages = list_all_stages(work_dir.root())?;

    // Load all sessions
    let sessions = load_all_sessions(work_dir)?;

    // Build stage summaries, then flatten the untrusted strings on them before
    // they reach a subscriber's terminal (see `sanitize`).
    let mut stage_summaries: Vec<StageSummary> = stages
        .iter()
        .map(|stage| build_stage_summary(stage, &sessions, work_dir))
        .collect();
    stage_summaries.iter_mut().for_each(sanitize_stage_summary);

    let merge_summary = collect_merge_summary(&stages, work_dir)?;

    // Calculate progress
    let progress = calculate_progress(&stages);

    // Load plan name (best-effort, don't fail status if unavailable)
    let mut plan_name = load_plan_name(work_dir);
    plan_name.iter_mut().for_each(neutralize);

    Ok(StatusData {
        stages: stage_summaries,
        merge: merge_summary,
        progress,
        plan_name,
        quota: crate::quota::read_snapshot(work_dir.root()),
        tokens: fresh_flow(work_dir),
        activity: crate::fs::stage_events::read_recent(
            work_dir.root(),
            crate::fs::stage_events::RECENT_LIMIT,
        ),
    })
}

/// The tracker's last flow when it is recent enough to show, sanitized at
/// this boundary (a planted `flow.json` is untrusted).
fn fresh_flow(work_dir: &WorkDir) -> Option<crate::tokens::TokenFlow> {
    let now = Utc::now().timestamp();
    let mut flow = crate::tokens::model::read_flow(work_dir.root())
        .filter(|flow| flow_is_fresh(flow.observed_at, now))?;
    sanitize_token_flow(&mut flow);
    Some(flow)
}
