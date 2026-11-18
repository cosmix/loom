//! Contracts for stage summary-engine: the summary defaults and the Codex summary command line.

use std::fs;

use loom::fs::work_dir::PlanConfig;
use loom::plan_server::summaries::codex_spawner::codex_summary_args;
use loom::planning::summary::config::SummaryModels;
use loom::user_config::UserConfig;
use tempfile::TempDir;

/// `UserConfig::load` with `LOOM_HOME` pointing at a home whose `config.toml` is empty.
fn empty_user_config() -> UserConfig {
    let home = TempDir::new().unwrap();
    fs::write(home.path().join("config.toml"), "").unwrap();
    let old = std::env::var_os("LOOM_HOME");
    std::env::set_var("LOOM_HOME", home.path());
    let user = UserConfig::load();
    match old {
        Some(value) => std::env::set_var("LOOM_HOME", value),
        None => std::env::remove_var("LOOM_HOME"),
    }
    user
}

#[test]
fn summary_models_default_to_haiku_for_documents_and_sonnet_for_overviews() {
    let resolved = SummaryModels::resolve(&PlanConfig::default(), &empty_user_config());
    assert_eq!(resolved.document_model, "haiku", "{resolved:?}");
    assert_eq!(resolved.document_effort, "high", "{resolved:?}");
    assert_eq!(resolved.overview_model, "sonnet", "{resolved:?}");
    assert_eq!(resolved.overview_effort, "high", "{resolved:?}");
    assert_eq!(resolved.backend, "claude", "{resolved:?}");
    assert_eq!(resolved.codex_model, "gpt-6.1-terra", "{resolved:?}");
    assert_eq!(resolved.codex_effort, "high", "{resolved:?}");
}

#[test]
fn codex_summary_uses_configured_model() {
    let tmp = TempDir::new().unwrap();
    let (root, runtime) = (tmp.path().join("project"), tmp.path().join("runtime"));
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&runtime).unwrap();
    let args = codex_summary_args("gpt-6-luna", "low", &root, &runtime, "prompt");
    assert!(args.iter().any(|arg| arg == "exec"), "{args:?}");
    let model = args
        .windows(2)
        .any(|pair| pair[0] == "-m" && pair[1] == "gpt-6-luna");
    assert!(model, "no `-m gpt-6-luna` in {args:?}");
    let effort = args.iter().any(|arg| arg == "model_reasoning_effort=low");
    assert!(effort, "no `model_reasoning_effort=low` in {args:?}");
    let root_text = root.to_string_lossy();
    let cwd = args
        .windows(2)
        .any(|pair| pair[0] == "-C" && pair[1] == root_text);
    assert!(cwd, "no `-C <root>` in {args:?}");
    let runtime_text = runtime.to_string_lossy();
    let granted = args.iter().any(|arg| arg.contains(runtime_text.as_ref()));
    assert!(granted, "the runtime directory is not granted in {args:?}");
    assert!(
        args.iter().any(|arg| arg == "prompt"),
        "no prompt in {args:?}"
    );
    let full_access = args.iter().any(|arg| arg.contains("danger-full-access"));
    assert!(!full_access, "the sandbox was widened in {args:?}");
}
