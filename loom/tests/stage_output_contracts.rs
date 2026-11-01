#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use loom::models::stage::Stage;
use loom::verify::transitions::save_stage;

#[test]
fn output_get_emits_a_json_value_for_scripting() {
    let workspace = tempfile::tempdir().unwrap();
    let work_dir = workspace.path().join(".loom/work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let mut stage = Stage::new("Example".into(), None);
    stage.id = "example".into();
    save_stage(&stage, &work_dir).unwrap();

    let value = "quote \" slash \\ newline\n tab\t λ";
    let run = |args: &[&str]| {
        helpers::loom_cmd()
            .args(args)
            .current_dir(workspace.path())
            .output()
            .unwrap()
    };
    let set = run(&["stage", "output", "set", "example", "key", value]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let get = run(&["stage", "output", "get", "example", "key"]);
    assert!(
        get.status.success(),
        "{}",
        String::from_utf8_lossy(&get.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&get.stdout).unwrap(),
        value
    );
}
