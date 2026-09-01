//! Canonical context-ceiling resolution for the `PostToolUse` shell hook.
//!
//! Shell is not a TOML parser. Keeping `[context]` parsing in awk made valid
//! inline/dotted tables disagree with the daemon and let fragments from some
//! invalid documents become live ceilings. This tiny internal command routes
//! the hook through the same Rust deserializer and stage override as every
//! other ceiling consumer.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::fs::work_dir::read_context_config;
use crate::validation::validate_id;

/// Print `<main>:<subagent>` or nothing when this is not a live Loom session.
///
/// A malformed config resolves to both built-in defaults, matching the daemon.
/// A missing, malformed, or mismatched stage record resolves the main ceiling
/// to zero (disabled): a hook must not judge a session against a ceiling whose
/// stage-specific tier it could not verify. The independent subagent ceiling
/// remains usable because it does not depend on the stage record.
pub fn context_ceilings() -> Result<()> {
    let Some((main, subagent)) = requested_pair() else {
        return Ok(());
    };
    println!("{main}:{subagent}");
    Ok(())
}

/// Resolve only from the exact state directory named by the hook protocol.
/// Interactive commands may search upward from a project path, but a stale
/// hook path below another checkout must not inherit that checkout's config.
fn requested_pair() -> Option<(u32, u32)> {
    let work_dir = PathBuf::from(std::env::var_os("LOOM_WORK_DIR")?);
    let stage_id = std::env::var("LOOM_STAGE_ID").ok()?;
    if !work_dir.is_dir() || validate_id(&stage_id).is_err() {
        return None;
    }
    Some(resolve(&work_dir, &stage_id))
}

fn resolve(work_dir: &Path, stage_id: &str) -> (u32, u32) {
    let config = read_context_config(work_dir).unwrap_or_default();
    let main = load_verified_stage_ceiling(work_dir, stage_id)
        .map(|stage_ceiling| config.ceiling_for(stage_ceiling))
        .unwrap_or(0);
    (main, config.subagent_ceiling_tokens)
}

/// `Ok(None)` means a verified stage with no override. Any lookup or identity
/// error disables the main hook ceiling instead of inventing a fallback.
fn load_verified_stage_ceiling(work_dir: &Path, stage_id: &str) -> Result<Option<u32>> {
    validate_id(stage_id)?;
    let stages_dir = work_dir.join("stages");
    let mut matches = 0usize;
    for entry in std::fs::read_dir(&stages_dir)? {
        let entry = entry?;
        let filename = entry.file_name();
        if crate::fs::stage_files::extract_stage_id(&filename.to_string_lossy()).as_deref()
            == Some(stage_id)
            && entry.path().extension().and_then(|ext| ext.to_str()) == Some("md")
        {
            matches += 1;
        }
    }
    anyhow::ensure!(matches == 1, "expected one stage record, found {matches}");
    let stage = crate::verify::load_stage(stage_id, work_dir)?;
    anyhow::ensure!(
        stage.id == stage_id,
        "stage record '{}' identifies itself as '{}'",
        stage_id,
        stage.id
    );
    Ok(stage.context_ceiling_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::work_dir::ContextConfig;
    use crate::models::constants::{
        DEFAULT_CONTEXT_CEILING_TOKENS, DEFAULT_SUBAGENT_CEILING_TOKENS,
    };
    use crate::models::stage::Stage;
    use crate::verify::serialize_stage_to_markdown;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn inline_and_dotted_context_tables_use_rust_toml_semantics() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            "context = { ceiling_tokens = 2000, subagent_ceiling_tokens = 1000 }\n",
        )
        .unwrap();
        write_stage(temp.path(), "stage-a", "stage-a", None);
        assert_eq!(resolve(temp.path(), "stage-a"), (2000, 1000));

        fs::write(
            temp.path().join("config.toml"),
            "context.ceiling_tokens = 3000\ncontext.subagent_ceiling_tokens = 1500\n",
        )
        .unwrap();
        assert_eq!(resolve(temp.path(), "stage-a"), (3000, 1500));
    }

    #[test]
    fn invalid_toml_uses_defaults_and_stage_override_still_wins() {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("config.toml"),
            "note = \"\"\"x\"\"\" trailing\n[context]\nceiling_tokens = 1\n",
        )
        .unwrap();
        write_stage(temp.path(), "stage-a", "stage-a", None);
        assert_eq!(
            resolve(temp.path(), "stage-a"),
            (
                DEFAULT_CONTEXT_CEILING_TOKENS,
                DEFAULT_SUBAGENT_CEILING_TOKENS
            )
        );

        write_stage(temp.path(), "stage-a", "stage-a", Some(777));
        assert_eq!(resolve(temp.path(), "stage-a").0, 777);
    }

    #[test]
    fn missing_malformed_and_mismatched_stages_disable_the_main_ceiling() {
        let temp = TempDir::new().unwrap();
        let defaults = ContextConfig::default();
        assert_eq!(
            resolve(temp.path(), "missing"),
            (0, defaults.subagent_ceiling_tokens)
        );

        fs::create_dir_all(temp.path().join("stages")).unwrap();
        fs::write(temp.path().join("stages/broken.md"), "not a stage").unwrap();
        assert_eq!(resolve(temp.path(), "broken").0, 0);

        write_stage(temp.path(), "wanted", "different", Some(1));
        assert_eq!(resolve(temp.path(), "wanted").0, 0);

        write_stage(temp.path(), "duplicate", "duplicate", Some(1));
        write_stage(temp.path(), "01-duplicate", "duplicate", Some(2));
        assert_eq!(resolve(temp.path(), "duplicate").0, 0);
    }

    #[test]
    #[serial]
    fn environment_path_is_exact_and_must_exist() {
        let temp = TempDir::new().unwrap();
        let real_work = temp.path().join(".loom").join("work");
        fs::create_dir_all(&real_work).unwrap();
        write_stage(&real_work, "stage-a", "stage-a", Some(777));

        std::env::set_var("LOOM_STAGE_ID", "stage-a");
        std::env::set_var("LOOM_WORK_DIR", &real_work);
        assert_eq!(
            requested_pair(),
            Some((777, DEFAULT_SUBAGENT_CEILING_TOKENS))
        );

        let linked_work = temp.path().join("linked-work");
        std::os::unix::fs::symlink(&real_work, &linked_work).unwrap();
        std::env::set_var("LOOM_WORK_DIR", &linked_work);
        assert_eq!(
            requested_pair(),
            Some((777, DEFAULT_SUBAGENT_CEILING_TOKENS))
        );

        // A nonexistent descendant must not search upward and rediscover the
        // real state directory above it.
        std::env::set_var("LOOM_WORK_DIR", real_work.join("missing"));
        assert_eq!(requested_pair(), None);
        std::env::remove_var("LOOM_STAGE_ID");
        std::env::remove_var("LOOM_WORK_DIR");
    }

    fn write_stage(work_dir: &Path, filename_id: &str, record_id: &str, ceiling: Option<u32>) {
        let mut stage = Stage::new("stage".to_string(), None);
        stage.id = record_id.to_string();
        stage.context_ceiling_tokens = ceiling;
        fs::create_dir_all(work_dir.join("stages")).unwrap();
        fs::write(
            work_dir.join(format!("stages/{filename_id}.md")),
            serialize_stage_to_markdown(&stage).unwrap(),
        )
        .unwrap();
    }
}
