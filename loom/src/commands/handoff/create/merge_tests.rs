use super::*;
use serial_test::serial;

struct DirectFixture {
    _guard: tests::CwdGuard,
    _temp: tempfile::TempDir,
    work_dir: std::path::PathBuf,
}

impl DirectFixture {
    fn new(stage_id: &str) -> Self {
        let guard = tests::CwdGuard::new();
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path().join(".loom").join("work");
        std::fs::create_dir_all(&work_dir).unwrap();
        std::fs::write(work_dir.join("config.toml"), "").unwrap();
        let mut stage = Stage::new(stage_id.to_string(), None);
        stage.id = stage_id.to_string();
        stage.status = StageStatus::Executing;
        crate::verify::transitions::create_stage(&stage, &work_dir).unwrap();
        env::set_current_dir(temp.path()).unwrap();
        Self {
            _guard: guard,
            _temp: temp,
            work_dir,
        }
    }
}

#[test]
#[serial]
fn a_second_identical_create_writes_no_new_artifact() {
    let fixture = DirectFixture::new("duplicate");
    for _ in 0..2 {
        execute_direct(
            Some("duplicate".to_string()),
            Some("same-session".to_string()),
            "session_end".to_string(),
            None,
        )
        .unwrap();
    }

    let count = std::fs::read_dir(fixture.work_dir.join("handoffs"))
        .unwrap()
        .count();
    assert_eq!(count, 1);
}

#[test]
#[serial]
fn ceiling_after_equal_session_end_writes_an_agent_ceiling_artifact() {
    let fixture = DirectFixture::new("ceiling-after-end");
    for trigger in ["session_end", CEILING_TRIGGER] {
        execute_direct(
            Some("ceiling-after-end".to_string()),
            Some("same-session".to_string()),
            trigger.to_string(),
            None,
        )
        .unwrap();
    }

    let path = fixture
        .work_dir
        .join("handoffs/ceiling-after-end-handoff-002.md");
    let parsed = crate::handoff::ParsedHandoff::parse(&std::fs::read_to_string(path).unwrap());
    assert_eq!(
        parsed.as_v2().unwrap().origin,
        Some(HandoffOrigin::AgentCeiling)
    );
}
