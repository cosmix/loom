//! Session kinds of a planning session and the layout of one session's runtime directory.

use std::path::{Component, Path, PathBuf};

use crate::planning::slug::{plan_home, plan_slug};

/// The purpose of a planning session.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionKind {
    /// A session that authors planning documents.
    Authoring,
    /// A session that summarizes planning work.
    Summary,
    /// A pressure session for a repository-relative `PLAN-<slug>.md` in a plan home.
    Pressure { plan: PathBuf },
}

impl SessionKind {
    /// Return the session kind's stable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Authoring => "authoring",
            Self::Summary => "summary",
            Self::Pressure { .. } => "pressure",
        }
    }

    /// Return the pressure plan's repository-relative brief bundle, if its name is valid.
    pub fn pressure_bundle(&self) -> Option<PathBuf> {
        let Self::Pressure { plan } = self else {
            return None;
        };
        let home = plan_home(plan)?;
        // Pressure targets are unarchived plans in a home, with no traversal components.
        if plan.parent()? != home.as_path()
            || plan
                .to_str()?
                .split('/')
                .any(|part| matches!(part, "." | ".."))
        {
            return None;
        }
        let name = plan.file_name()?.to_str()?;
        if !name.starts_with("PLAN-") {
            return None;
        }
        let slug = plan_slug(name)?;
        // `PLAN-..md` has the slug `.`, which would name the home's `briefs` directory itself.
        let mut parts = Path::new(&slug).components();
        let one_name = matches!(
            (parts.next(), parts.next()),
            (Some(Component::Normal(part)), None) if part == slug.as_str()
        );
        one_name.then(|| home.join("briefs").join(slug))
    }
}

/// One session's runtime directory, created by the session bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDir(PathBuf);

impl SessionDir {
    /// Wrap a session directory path.
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }

    /// Return the session directory path.
    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Return the settings file path.
    pub fn settings(&self) -> PathBuf {
        self.0.join("settings.json")
    }

    /// Return the guard policy file path.
    pub fn guard(&self) -> PathBuf {
        self.0.join("guard.json")
    }

    /// Return the session record path.
    pub fn record(&self) -> PathBuf {
        self.0.join("session.json")
    }

    /// Return the launch script path.
    pub fn launch_script(&self) -> PathBuf {
        self.0.join("launch.sh")
    }

    /// Return the session brief path.
    pub fn brief(&self) -> PathBuf {
        self.0.join("brief.md")
    }

    /// Return the session plugin directory path.
    pub fn plugin(&self) -> PathBuf {
        self.0.join("plugin")
    }

    /// Return the event spool path.
    pub fn events(&self) -> PathBuf {
        self.0.join("events.jsonl")
    }

    /// Return the answers directory path.
    pub fn answers(&self) -> PathBuf {
        self.0.join("answers")
    }

    /// Return the completion directory path.
    pub fn done(&self) -> PathBuf {
        self.0.join("done")
    }

    /// Return the completion marker path.
    pub fn marker(&self) -> PathBuf {
        self.done().join("complete")
    }

    /// Return the Codex log path.
    pub fn codex_log(&self) -> PathBuf {
        self.0.join("codex.log")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionDir {
        SessionDir::new(PathBuf::from("/runtime/sessions/example"))
    }

    #[test]
    fn labels() {
        assert_eq!(SessionKind::Authoring.label(), "authoring");
        assert_eq!(SessionKind::Summary.label(), "summary");
        assert_eq!(
            SessionKind::Pressure {
                plan: "doc/plans/PLAN-x.md".into()
            }
            .label(),
            "pressure"
        );
    }

    #[test]
    fn pressure_bundle() {
        let pressure = |plan: &str| SessionKind::Pressure { plan: plan.into() };
        assert_eq!(
            pressure("doc/plans/PLAN-x.md").pressure_bundle(),
            Some("doc/plans/briefs/x".into())
        );
        assert_eq!(
            pressure("doc/plans/alpha/PLAN-z.md").pressure_bundle(),
            Some("doc/plans/alpha/briefs/z".into())
        );
        assert_eq!(pressure("doc/plans/other.md").pressure_bundle(), None);
        assert_eq!(pressure("doc/plans/DONE-PLAN-x.md").pressure_bundle(), None);
        assert_eq!(SessionKind::Authoring.pressure_bundle(), None);
        assert_eq!(SessionKind::Summary.pressure_bundle(), None);
    }

    #[test]
    fn pressure_bundle_refuses_a_slug_that_is_not_one_plain_name() {
        let pressure = |plan: &str| SessionKind::Pressure { plan: plan.into() };
        // Slugs `.` and `..` would name `doc/plans/briefs` or its parent.
        for home in ["doc/plans", "doc/plans/alpha"] {
            for name in ["PLAN-..md", "PLAN-...md"] {
                let plan = format!("{home}/{name}");
                assert_eq!(pressure(&plan).pressure_bundle(), None, "{plan}");
            }
        }
        assert_eq!(
            pressure("doc/plans/PLAN-x.y.md").pressure_bundle(),
            Some("doc/plans/briefs/x.y".into())
        );
    }

    #[test]
    fn pressure_bundle_refuses_invalid_homes() {
        for home in [
            "doc/plans/Bad",
            "doc/plans/-bad",
            "doc/plans/bad_name",
            "doc/plans/briefs",
            "doc/plans/archive",
            "doc/plans/plans",
            "doc/plans/overview",
            "doc/plans/alpha/nested",
            "doc/plans/alpha/archive",
            "doc/plans/.",
            "doc/plans/..",
            "elsewhere",
            "/doc/plans",
        ] {
            let kind = SessionKind::Pressure {
                plan: format!("{home}/PLAN-z.md").into(),
            };
            assert_eq!(kind.pressure_bundle(), None, "{home}");
        }
    }

    #[test]
    fn pressure_serialization() {
        let kind = SessionKind::Pressure {
            plan: "doc/plans/PLAN-x.md".into(),
        };
        let value = serde_json::json!({"kind": "pressure", "plan": "doc/plans/PLAN-x.md"});
        assert_eq!(serde_json::to_value(&kind).unwrap(), value);
        assert_eq!(serde_json::from_value::<SessionKind>(value).unwrap(), kind);
    }

    #[test]
    fn new() {
        assert_eq!(
            SessionDir::new("example".into()).path(),
            Path::new("example")
        );
    }

    #[test]
    fn path() {
        assert_eq!(session().path(), Path::new("/runtime/sessions/example"));
    }

    #[test]
    fn settings() {
        assert_eq!(session().settings(), session().path().join("settings.json"));
    }
    #[test]
    fn guard() {
        assert_eq!(session().guard(), session().path().join("guard.json"));
    }
    #[test]
    fn record() {
        assert_eq!(session().record(), session().path().join("session.json"));
    }
    #[test]
    fn launch_script() {
        assert_eq!(
            session().launch_script(),
            session().path().join("launch.sh")
        );
    }
    #[test]
    fn brief() {
        assert_eq!(session().brief(), session().path().join("brief.md"));
    }
    #[test]
    fn plugin() {
        assert_eq!(session().plugin(), session().path().join("plugin"));
    }
    #[test]
    fn events() {
        assert_eq!(session().events(), session().path().join("events.jsonl"));
    }
    #[test]
    fn answers() {
        assert_eq!(session().answers(), session().path().join("answers"));
    }
    #[test]
    fn done() {
        assert_eq!(session().done(), session().path().join("done"));
    }
    #[test]
    fn marker() {
        assert_eq!(session().marker(), session().path().join("done/complete"));
    }
    #[test]
    fn codex_log() {
        assert_eq!(session().codex_log(), session().path().join("codex.log"));
    }
}
