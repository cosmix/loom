use super::*;
use crate::commands::status::render::{attention::render_attention, compact::render_compact};

#[test]
fn collected_names_are_safe_in_payload_and_terminal_renderers() {
    let temp = tempfile::tempdir().unwrap();
    let work = WorkDir::new(temp.path()).unwrap();
    work.initialize().unwrap();
    fs::write(
        temp.path().join("plan.md"),
        "# Plan Ω \x1b]52;c;payload\x07 \x1b[31m\u{202e}終\n",
    )
    .unwrap();
    fs::write(
        work.root().join("config.toml"),
        "[plan]\nsource_path = 'plan.md'\n",
    )
    .unwrap();
    let stage = Stage {
        id: "stage-1".into(),
        name: "Stage Ω \x1b]52;c;payload\x07 \x1b[31m\u{202e}終".into(),
        status: StageStatus::Blocked,
        ..Stage::default()
    };
    crate::verify::transitions::save_stage(&stage, work.root()).unwrap();

    let data = collect_status_data(&work).unwrap();
    assert_eq!(data.stages[0].id, "stage-1");
    for value in [
        data.plan_name.as_deref().unwrap(),
        data.stages[0].name.as_str(),
    ] {
        assert!(value.contains('Ω') && value.contains('終'));
        assert!(!value.contains(['\x1b', '\x07', '\u{202e}']));
    }
    let payload = serde_json::to_string(&data).unwrap();
    assert!(!payload.contains("\\u001b"));
    let mut output = Vec::new();
    render_compact(&mut output, &data).unwrap();
    render_attention(&mut output, &data.stages, false).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("Plan Ω") && output.contains("Stage Ω"));
    assert!(!output.contains(['\x1b', '\x07', '\u{202e}']));
}
