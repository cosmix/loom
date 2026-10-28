//! The guidance seam for authoring chats: the provider trait, its slot in the extensions and the fixed provider.

use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock};

use anyhow::Result;

use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_open_dirfd, safe_write_with_mode_in_workdir,
};
use crate::plan_server::context::PlanContext;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::SessionDir;

const BRIEF: &str = "You are a planning session started by `loom plan`. Write only under doc/. Ask in plain text, never with a question tool.";
const PLUGIN: &str =
    r#"{"name": "loom-plan", "version": "0.0.0", "description": "loom plan session"}"#;
const CLOSING: &str = "Record the remaining open questions and anything still pending into the documents now, then stop.";

/// Supplies launch files and messages without coupling guidance to the bridge.
pub trait GuidanceProvider: Send + Sync {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
    ) -> Result<LaunchGuidance>;

    fn closing_message(&self) -> String;
}

/// The current guidance provider, replaceable through the server's extensions.
pub struct GuidanceSlot(RwLock<Arc<dyn GuidanceProvider>>);

impl GuidanceSlot {
    pub fn new(provider: Arc<dyn GuidanceProvider>) -> Self {
        Self(RwLock::new(provider))
    }

    pub fn get(&self) -> Arc<dyn GuidanceProvider> {
        Arc::clone(&self.0.read().unwrap_or_else(PoisonError::into_inner))
    }

    pub fn set(&self, provider: Arc<dyn GuidanceProvider>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = provider;
    }
}

/// Fixed authoring guidance until the guidance engine installs its provider.
pub struct FixedGuidance;

impl GuidanceProvider for FixedGuidance {
    fn authoring(
        &self,
        _ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
    ) -> Result<LaunchGuidance> {
        let fd = safe_open_dirfd(dir.path())?;
        safe_write_with_mode_in_workdir(
            fd.as_raw_fd(),
            Path::new("brief.md"),
            BRIEF.as_bytes(),
            0o600,
        )?;
        safe_create_dir_all_in_workdir(fd.as_raw_fd(), Path::new("plugin/.claude-plugin"), 0o700)?;
        safe_write_with_mode_in_workdir(
            fd.as_raw_fd(),
            Path::new("plugin/.claude-plugin/plugin.json"),
            PLUGIN.as_bytes(),
            0o600,
        )?;
        // Preserve a delivered message verbatim; blank input still needs a usable opening.
        let opening_message = match first_message.filter(|text| !text.trim().is_empty()) {
            Some(text) => text.to_owned(),
            None => match offer {
                Some(id) => format!("Start the step \"{id}\"."),
                None => "Start a planning session.".to_owned(),
            },
        };
        Ok(LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message,
            model: "opus".to_owned(),
            effort: "medium".to_owned(),
            display_name: "loom plan: chat".to_owned(),
            opening_on_argv: false,
        })
    }

    fn closing_message(&self) -> String {
        CLOSING.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::{symlink, PermissionsExt};

    use tempfile::TempDir;

    use super::*;
    use crate::plan_server::context::BindInfo;
    use crate::plan_server::prereq::ReadOnly;
    use crate::planning::watch::OwnWrites;

    fn fixture() -> (TempDir, PlanContext, SessionDir) {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("doc/product")).unwrap();
        fs::write(root.path().join("doc/product/index.md"), "# Demo\n").unwrap();
        let bind = BindInfo::new("127.0.0.1:7272".parse().unwrap(), &"ab".repeat(32));
        let ctx = PlanContext::new(
            root.path().to_path_buf(),
            ReadOnly::from_problems(Vec::new()),
            bind,
            OwnWrites::default(),
        )
        .unwrap();
        let dir = SessionDir::new(root.path().join("session"));
        safe_create_dir_all_in_workdir(
            safe_open_dirfd(root.path()).unwrap().as_raw_fd(),
            Path::new("session"),
            0o700,
        )
        .unwrap();
        (root, ctx, dir)
    }

    #[test]
    fn first_message_is_verbatim_and_takes_precedence_over_offer() {
        let (_root, ctx, dir) = fixture();
        let message = "  Plan the next change.\n@doc/product/index.md\n";
        let guidance = FixedGuidance
            .authoring(&ctx, &dir, Some("offer-id"), Some(message))
            .unwrap();
        assert_eq!(guidance.opening_message, message);
        assert!(!guidance.opening_on_argv);
    }

    #[test]
    fn offer_opens_its_step() {
        let (_root, ctx, dir) = fixture();
        let guidance = FixedGuidance
            .authoring(&ctx, &dir, Some("offer-id"), None)
            .unwrap();
        assert_eq!(guidance.opening_message, "Start the step \"offer-id\".");
    }

    #[test]
    fn new_chat_and_blank_messages_have_a_nonempty_opening() {
        let (_root, ctx, dir) = fixture();
        for message in [None, Some(""), Some(" \n\t")] {
            let guidance = FixedGuidance.authoring(&ctx, &dir, None, message).unwrap();
            assert_eq!(guidance.opening_message, "Start a planning session.");
        }
    }

    #[test]
    fn writes_private_brief_and_minimal_plugin_with_fixed_launch_settings() {
        let (_root, ctx, dir) = fixture();
        let guidance = FixedGuidance.authoring(&ctx, &dir, None, None).unwrap();
        let plugin = dir.plugin().join(".claude-plugin/plugin.json");
        assert_eq!(guidance.brief_path, dir.brief());
        assert_eq!(guidance.plugin_dir, dir.plugin());
        assert_eq!(fs::read_to_string(dir.brief()).unwrap(), BRIEF);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&plugin).unwrap())
                .unwrap(),
            serde_json::json!({
                "name": "loom-plan", "version": "0.0.0", "description": "loom plan session"
            })
        );
        for path in [dir.brief(), plugin] {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            FixedGuidance.authoring(&ctx, &dir, None, None).unwrap();
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(guidance.model, "opus");
        assert_eq!(guidance.effort, "medium");
        assert_eq!(guidance.display_name, "loom plan: chat");
        assert!(!guidance.opening_on_argv);
        assert_eq!(FixedGuidance.closing_message(), CLOSING);
    }

    #[test]
    fn refuses_a_symlinked_plugin_directory() {
        let (_root, ctx, dir) = fixture();
        let outside = tempfile::tempdir().unwrap();
        symlink(outside.path(), dir.plugin()).unwrap();
        assert!(FixedGuidance.authoring(&ctx, &dir, None, None).is_err());
        assert!(!outside.path().join(".claude-plugin").exists());
    }

    #[test]
    fn slot_replaces_provider_without_invalidating_an_existing_snapshot() {
        let initial: Arc<dyn GuidanceProvider> = Arc::new(FixedGuidance);
        let replacement: Arc<dyn GuidanceProvider> = Arc::new(FixedGuidance);
        let slot = GuidanceSlot::new(Arc::clone(&initial));
        let snapshot = slot.get();
        slot.set(Arc::clone(&replacement));
        assert!(Arc::ptr_eq(&snapshot, &initial));
        assert!(Arc::ptr_eq(&slot.get(), &replacement));
    }
}
