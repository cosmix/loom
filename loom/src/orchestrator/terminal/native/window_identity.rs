//! Exact native window identity, separate from the stage label used by tmux.

use sha2::{Digest, Sha256};
use std::path::Path;

use crate::models::session::Session;

use super::NativeBackend;

/// Persist the result at launch; legacy records without it must not guess a
/// stage window during teardown.
pub(super) fn native_window_title(work_dir: &Path, session: &Session) -> Option<String> {
    let (tracking_key, _) = NativeBackend::window_title_and_pid_key(session)?;
    let project_path = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    let digest = Sha256::digest(project_path.as_os_str().as_encoded_bytes());
    let project = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Some(format!("{tracking_key}-{project}-{}", session.id))
}

/// A persisted title is usable only for the session and project that created
/// it. Mismatched or legacy records must use guarded PID evidence.
pub(super) fn attributable_title<'a>(work_dir: &Path, session: &'a Session) -> Option<&'a str> {
    let title = session.native_window_title.as_deref()?;
    (native_window_title(work_dir, session).as_deref() == Some(title)).then_some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage_session(stage_id: &str) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session
    }

    #[test]
    fn identity_separates_generations_and_projects() {
        let mut first = stage_session("shared-stage");
        let mut replacement = stage_session("shared-stage");
        replacement.id = format!("{}-replacement", first.id);
        let project_a = Path::new("/project-a/.loom/work");
        let project_b = Path::new("/project-b/.loom/work");

        let first_title = native_window_title(project_a, &first).unwrap();
        let replacement_title = native_window_title(project_a, &replacement).unwrap();
        let other_project_title = native_window_title(project_b, &first).unwrap();
        assert_ne!(first_title, replacement_title);
        assert_ne!(first_title, other_project_title);
        assert!(first_title.starts_with("loom-v2-stage-shared-stage-"));
        assert!(first_title.ends_with(&first.id));

        first.native_window_title = Some(first_title.clone());
        assert_eq!(
            attributable_title(project_a, &first),
            Some(first_title.as_str())
        );
        assert_eq!(attributable_title(project_b, &first), None);
        let restored: Session =
            serde_json::from_value(serde_json::to_value(&first).unwrap()).unwrap();
        assert_eq!(
            restored.native_window_title.as_deref(),
            Some(first_title.as_str())
        );
    }

    #[test]
    fn legacy_session_has_no_attributable_window() {
        let session = stage_session("shared-stage");
        let mut persisted = serde_json::to_value(session).unwrap();
        persisted
            .as_object_mut()
            .unwrap()
            .remove("native_window_title");
        let restored: Session = serde_json::from_value(persisted).unwrap();
        assert!(restored.native_window_title.is_none());
        assert!(attributable_title(Path::new("/project-a"), &restored).is_none());
    }
}
