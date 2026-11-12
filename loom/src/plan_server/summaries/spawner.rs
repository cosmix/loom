//! The real `SessionSpawner` over the session bridge.

use std::sync::{Arc, Weak};

use anyhow::{anyhow, Context};

use super::scheduler::SessionSpawner;
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle::{line, safe};
use crate::plan_session::bridge::{Bridge, EndCause, EndMode, GuidanceFn, StartRequest};
use crate::plan_session::kind::SessionKind;
use crate::planning::guidance::{render_session_guidance, GuidanceModels, SessionGuidanceKind};
use crate::planning::project::Project;
use crate::planning::summary::claims::clear_session_claims;
use crate::planning::summary::store::validate_id;

/// Starts summary sessions through the bridge in `ctx.extensions()`.
pub struct RealSpawner {
    ctx: Weak<PlanContext>,
}

impl RealSpawner {
    pub fn new(ctx: &Arc<PlanContext>) -> Self {
        RealSpawner {
            ctx: Arc::downgrade(ctx),
        }
    }

    fn parts(&self) -> anyhow::Result<(Arc<PlanContext>, Arc<Bridge>)> {
        let ctx = self.ctx.upgrade().context("the planning server is gone")?;
        let bridge = ctx.extensions().get::<Bridge>();
        let bridge = bridge.context("the planning server runs no session bridge")?;
        Ok((ctx, bridge))
    }
}

impl SessionSpawner for RealSpawner {
    /// A summary session has no chat: no offer, no opening origin. Plan 8 renders its guidance
    /// from a snapshot of the project and the server's runtime directory, never re-resolved
    /// (`LOOM_PLAN_STATE` can move it).
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        let (ctx, bridge) = self.parts()?;
        // The bridge's drain final-writes the sessions it ends, so a session started after the
        // first stop signal would be cut short; the panel shows this message as the start error.
        if ctx.is_stopping() {
            return Err(anyhow!(
                "loom plan is stopping; no new summary session starts"
            ));
        }
        let project = ctx.project().clone();
        let runtime = ctx.runtime().clone();
        let root = ctx.root().to_path_buf();
        let feature = batch_feature(&project, ids).map(str::to_owned);
        let kind = SessionGuidanceKind::Summary { ids: ids.to_vec() };
        let guidance: GuidanceFn = Box::new(move |dir| {
            let models = GuidanceModels::load(&root)?;
            render_session_guidance(&kind, &project, &runtime, dir, &models, feature.as_deref())
                .map(Into::into)
        });
        let request = StartRequest {
            kind: SessionKind::Summary,
            guidance,
            opening_origin: None,
            offer: None,
            feature: None,
            plan: None,
            extra_env: Vec::new(),
        };
        bridge
            .start_session(request)
            .map_err(|error| anyhow!(error))
    }

    /// Tear the session down; a refusal is printed and the session counts as running until its
    /// `Ended` arrives. A summary session queues nothing, so the returned queue is dropped.
    fn stop(&self, session: &str, release_claims: bool) {
        let (ctx, bridge) = match self.parts() {
            Ok(parts) => parts,
            Err(error) => {
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "summaries: cannot stop session {}: {why}",
                    safe(session)
                ));
                return;
            }
        };
        if let Err(error) = bridge.end(session, EndMode::Teardown, EndCause::Caller) {
            let why = safe(&error.to_string());
            line(&format!(
                "summaries: cannot stop session {}: {why}",
                safe(session)
            ));
        }
        if release_claims {
            if let Err(error) = clear_session_claims(ctx.runtime(), session) {
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "summaries: cannot release the claims of {}: {why}",
                    safe(session)
                ));
            }
        }
    }
}

fn summary_feature<'a>(project: &'a Project, id: &'a str) -> Option<&'a str> {
    validate_id(id).ok()?;
    if let Some(slug) = id.strip_prefix("plans/") {
        return project.tree.plan(slug)?.feature.as_deref();
    }
    let scoped = id.strip_prefix("overview/").unwrap_or(id);
    let (feature, _) = scoped.split_once('/')?;
    project.feature(feature).map(|_| feature)
}

fn batch_feature<'a>(project: &'a Project, ids: &'a [String]) -> Option<&'a str> {
    let feature = summary_feature(project, ids.first()?)?;
    // The scheduler can batch several features and standalone plans into one session.
    ids.iter()
        .all(|id| summary_feature(project, id) == Some(feature))
        .then_some(feature)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{batch_feature, summary_feature, RealSpawner, SessionSpawner};
    use crate::plan_server::context::{BindInfo, PlanContext};
    use crate::plan_server::prereq::ReadOnly;
    use crate::plan_session::bridge::Bridge;
    use crate::planning::project::Project;
    use crate::planning::runtime::runtime_paths_under;
    use crate::planning::watch::OwnWrites;

    fn fixture() -> (tempfile::TempDir, Project) {
        let temp = tempfile::tempdir().unwrap();
        for path in [
            "doc/product/demo/section_01.md",
            "doc/plans/demo/PLAN-a.md",
            "doc/plans/demo/archive/DONE-PLAN-archived.md",
            "doc/plans/other/PLAN-b.md",
            "doc/plans/PLAN-solo.md",
        ] {
            let path = temp.path().join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "# Document\nImplements: none\n").unwrap();
        }
        let project = Project::load(temp.path()).unwrap();
        (temp, project)
    }

    #[test]
    fn installed_spawner_without_a_test_binary_refuses_before_reserving_a_session() {
        let (temp, _) = fixture();
        let runtime = runtime_paths_under(&temp.path().join("runtime"), temp.path()).unwrap();
        let ctx = PlanContext::new(
            temp.path().to_path_buf(),
            ReadOnly::from_problems(Vec::new()),
            BindInfo::new("127.0.0.1:0".parse().unwrap(), &"ab".repeat(32)),
            OwnWrites::default(),
        )
        .unwrap()
        .with_runtime(runtime);
        let ctx = Arc::new(ctx);
        crate::plan_server::chat::install(&ctx);

        let error = RealSpawner::new(&ctx)
            .start_summary(&["demo/product/section_01".to_owned()])
            .expect_err("an installed test spawner must not launch the real claude");

        assert_eq!(error.to_string(), "no claude binary is configured");
        let bridge = ctx.extensions().get::<Bridge>().unwrap();
        assert!(
            bridge.sessions().is_empty(),
            "no process can be queued to launch"
        );
        assert!(
            !ctx.runtime().sessions.exists(),
            "no session directory was created"
        );
    }

    #[test]
    fn summary_ids_select_their_own_features() {
        let (_temp, project) = fixture();

        for (id, expected) in [
            ("overview/demo/product", Some("demo")),
            ("demo/product/section_01", Some("demo")),
            ("demo/metaplan/index", Some("demo")),
            ("plans/a", Some("demo")),
            ("plans/archived", Some("demo")),
            ("plans/b", Some("other")),
            ("plans/solo", None),
            ("plans/missing", None),
            ("overview/missing/product", None),
            ("overview/demo/invalid", None),
        ] {
            assert_eq!(summary_feature(&project, id), expected, "{id}");
        }
    }

    #[test]
    fn summary_batch_is_scoped_only_when_all_ids_share_a_feature() {
        let (_temp, project) = fixture();

        for (ids, expected) in [
            (
                vec![
                    "overview/demo/product",
                    "demo/product/section_01",
                    "plans/a",
                ],
                Some("demo"),
            ),
            (vec!["plans/a", "plans/b"], None),
            (vec!["plans/a", "plans/solo"], None),
            (vec!["plans/solo", "plans/a"], None),
            (vec!["plans/solo"], None),
            (vec![], None),
        ] {
            let ids: Vec<String> = ids.into_iter().map(str::to_owned).collect();
            assert_eq!(batch_feature(&project, &ids), expected, "{ids:?}");
        }
    }
}
