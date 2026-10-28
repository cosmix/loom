use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;
use tempfile::TempDir;

use super::{PlanGuidance, SessionBriefView};
use crate::fs::work_dir::PlanConfig;
use crate::plan_server::chat::guidance::GuidanceProvider;
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, PublicId};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::request::{ApiRequest, ApiResponse, QueryParams};
use crate::plan_server::routes::{BodyKind, Method, Role};
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::SessionDir;
use crate::planning::checklist::Offer;
use crate::planning::guidance::{GuidanceModels, ModelEffort, StartBlocks};
use crate::planning::watch::OwnWrites;
use crate::user_config::UserConfig;
use crate::web_server::http::parse_head;

mod install;

struct HomeGuard(Option<OsString>);

impl HomeGuard {
    fn set(home: &Path) -> Self {
        let previous = std::env::var_os("LOOM_HOME");
        std::env::set_var("LOOM_HOME", home);
        Self(previous)
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.0 {
            Some(value) => std::env::set_var("LOOM_HOME", value),
            None => std::env::remove_var("LOOM_HOME"),
        }
    }
}

struct Fixture {
    ctx: PlanContext,
    provider: Arc<PlanGuidance>,
    session: SessionDir,
    _home: HomeGuard,
    _temp: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = HomeGuard::set(&temp.path().join("home"));
        let root = temp.path().join("project");
        project_files(&root);
        let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
        let ctx = PlanContext::new(
            root,
            ReadOnly::from_problems(Vec::new()),
            bind,
            OwnWrites::default(),
        )
        .unwrap();
        let provider = Arc::new(PlanGuidance::new(models()));
        ctx.extensions().insert(Arc::clone(&provider));
        let path = temp.path().join("rendered-session");
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        Self {
            ctx,
            provider,
            session: SessionDir::new(path),
            _home: home,
            _temp: temp,
        }
    }

    fn render(&self, offer: Option<&str>, message: Option<&str>) -> anyhow::Result<LaunchGuidance> {
        self.provider
            .authoring(&self.ctx, &self.session, offer, message)
    }
}

fn project_files(root: &Path) {
    put(
        root,
        "doc/product/index.md",
        concat!(
            "# Vision\n\n## Specification sections\n\n",
            "| # | Section |\n| --- | --- |\n| 01 | Alpha |\n",
        ),
    );
    put(root, "doc/product/section_01.md", "# Alpha\n\nBody.\n");
    put(
        root,
        "doc/plans/index.md",
        "# Metaplan\n\n## 1. Delivery (`PLAN-delivery.md`)\n\nDeliver it.\n",
    );
}

fn put(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn models() -> GuidanceModels {
    let mut models = GuidanceModels::resolve(&PlanConfig::default(), &UserConfig::default());
    models.spec = ModelEffort {
        model: "spec-model".into(),
        effort: "medium".into(),
    };
    models.plan = ModelEffort {
        model: "plan-model".into(),
        effort: "high".into(),
    };
    models
}

fn brief_response(ctx: &PlanContext, session_id: &str) -> ApiResponse {
    let route = super::routes().pop().unwrap();
    assert_eq!(route.path, "/api/session/brief");
    assert_eq!(
        (route.method, route.body, route.role),
        (Method::Get, BodyKind::None, Role::Viewer)
    );
    let raw =
        format!("GET /api/session/brief?session={session_id} HTTP/1.1\r\nHost: localhost\r\n\r\n",);
    let head = parse_head(raw.as_bytes()).unwrap().unwrap();
    let browser = BrowserId::parse(&"ab".repeat(16)).unwrap();
    let public_id = PublicId::mint();
    let request = ApiRequest {
        ctx,
        head: &head,
        browser: &browser,
        public_id: &public_id,
        query: QueryParams::parse(head.query.as_deref()).unwrap(),
        body: Vec::new(),
    };
    (route.handler)(&request)
}

#[test]
#[serial_test::serial]
fn write_plan_offer_renders_with_plan_step_model() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.ctx.project().checklist.primary,
        Some(Offer::WritePlan {
            metaplan: "doc/plans/index.md".into(),
            slug: "delivery".into(),
        })
    );

    let guidance = fixture.render(Some("write-plan:delivery"), None).unwrap();

    assert_eq!(guidance.model, "plan-model");
    assert_eq!(guidance.effort, "high");
    assert_eq!(guidance.brief_path, fixture.session.brief());
    assert!(!fs::read_to_string(guidance.brief_path).unwrap().is_empty());
    assert!(guidance
        .plugin_dir
        .join(".claude-plugin/plugin.json")
        .is_file());
    assert!(!guidance.opening_on_argv);
}

#[test]
#[serial_test::serial]
fn unknown_offer_error_names_the_id() {
    let fixture = Fixture::new();
    let id = "unknown-offer:missing";

    let error = fixture.render(Some(id), None).unwrap_err();

    assert!(error.to_string().contains(id), "{error:#}");
    assert!(!fixture.session.brief().exists());
}

#[test]
#[serial_test::serial]
fn plan_attachment_selects_plan_model_and_preserves_formatted_opening() {
    let fixture = Fixture::new();
    let message = concat!(
        "  Discuss this plan.\nKeep this line.\n\n<loom-attachments>\n",
        "[1] source doc/plans/index.md\n> # Metaplan\n</loom-attachments>",
    );

    let guidance = fixture.render(None, Some(message)).unwrap();

    assert_eq!(guidance.model, "plan-model");
    assert_eq!(guidance.effort, "high");
    assert_eq!(guidance.opening_message, message);
    assert!(!guidance.opening_on_argv);
}

#[test]
#[serial_test::serial]
fn product_attachment_selects_spec_model() {
    let fixture = Fixture::new();
    let message = concat!(
        "Discuss this section.\n\n<loom-attachments>\n",
        "[1] source doc/product/section_01.md\n> # Alpha\n</loom-attachments>",
    );

    let guidance = fixture.render(None, Some(message)).unwrap();

    assert_eq!(guidance.model, "spec-model");
    assert_eq!(guidance.effort, "medium");
    assert_eq!(guidance.opening_message, message);
}

#[test]
#[serial_test::serial]
fn brief_route_unknown_session_returns_404() {
    let fixture = Fixture::new();

    let response = brief_response(&fixture.ctx, "unknown-session");

    assert_eq!(response.status, 404);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["error"], "unknown_session");
}

#[test]
#[serial_test::serial]
fn brief_route_returns_rendered_session_brief() {
    let fixture = Fixture::new();
    let guidance = fixture.render(Some("write-plan:delivery"), None).unwrap();
    let brief = fs::read_to_string(&guidance.brief_path).unwrap();
    assert!(!brief.is_empty());

    let response = brief_response(&fixture.ctx, "rendered-session");

    assert_eq!(response.status, 200);
    let view: SessionBriefView = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(
        view,
        SessionBriefView {
            session_id: "rendered-session".into(),
            label: "Write PLAN-delivery.md".into(),
            brief,
            model: "plan-model".into(),
            effort: "high".into(),
            display_name: guidance.display_name,
        }
    );
    assert_eq!(
        brief_response(&fixture.ctx, "rendered-session-extra").status,
        404
    );
}

fn assert_not_offered(ctx: &PlanContext, id: &str) {
    let project = ctx.project();
    assert!(!project
        .checklist
        .primary
        .iter()
        .chain(project.checklist.secondary.iter())
        .any(|offer| offer.id() == id));
}

#[test]
#[serial_test::serial]
fn refine_existing_document_is_accepted_without_a_checklist_offer() {
    let fixture = Fixture::new();
    let id = "refine:plans/index.md";
    assert_not_offered(&fixture.ctx, id);

    let guidance = fixture.render(Some(id), None).unwrap();

    assert!(guidance
        .opening_message
        .contains("Refine doc/plans/index.md."));
    assert_eq!(guidance.model, "plan-model");
}

#[test]
#[serial_test::serial]
fn offer_with_typed_message_appends_the_message_to_the_generated_opening() {
    let fixture = Fixture::new();
    let message = concat!(
        "Tighten this passage.\n\n<loom-attachments>\n",
        "[1] source doc/product/section_01.md\n> # Alpha\n</loom-attachments>",
    );

    let guidance = fixture
        .render(Some("refine-passage:product/section_01.md"), Some(message))
        .unwrap();

    let generated = guidance
        .opening_message
        .strip_suffix(message)
        .and_then(|head| head.strip_suffix("\n\n"))
        .expect("opening ends with a blank line and the typed message");
    assert!(
        generated.contains("Refine doc/product/section_01.md."),
        "{generated}"
    );
}

#[test]
#[serial_test::serial]
fn knowledge_is_accepted_without_a_checklist_offer() {
    let fixture = Fixture::new();
    assert_not_offered(&fixture.ctx, "knowledge");

    let guidance = fixture.render(Some("knowledge"), None).unwrap();

    assert!(guidance.opening_message.contains("loom-plan:knowledge"));
    assert_eq!(guidance.model, "spec-model");
}

#[test]
#[serial_test::serial]
fn start_block_refuses_a_provider_created_after_a_restart() {
    let reason = "session hooks changed; review configuration before starting";
    let fixture = Fixture::new();
    StartBlocks::block(fixture.ctx.runtime(), reason, &[]).unwrap();
    // A restarted server has a provider with nothing in memory; the block lives on disk.
    let restarted = PlanGuidance::new(models());

    let error = restarted
        .authoring(&fixture.ctx, &fixture.session, None, None)
        .unwrap_err();

    assert!(format!("{error:#}").contains(reason), "{error:#}");
    assert!(!fixture.session.brief().exists());
    assert!(!fixture.session.plugin().exists());
}
