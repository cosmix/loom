use super::*;
use tempfile::TempDir;

fn sample_flow() -> TokenFlow {
    let usage = TokenUsage {
        input: 10,
        cache_write: 20,
        cache_read: 30,
        output: 40,
    };
    TokenFlow {
        observed_at: 1_788_523_200,
        bucket_secs: BUCKET_SECS,
        plan: PlanFlow {
            total: usage,
            rate: usage,
            series: vec![70; PLAN_SERIES_LEN],
        },
        stages: vec![StageFlow {
            stage_id: "server".into(),
            total: usage,
            rate: usage,
            series: vec![70; STAGE_SERIES_LEN],
            hit_series: Vec::new(),
            agents: vec![AgentFlow {
                id: "main-01".into(),
                kind: AgentKind::Main,
                agent_type: None,
                model: Some("claude-opus-5-5".into()),
                active: true,
                total: usage,
                rate: usage,
                series: vec![70; STAGE_SERIES_LEN],
            }],
        }],
        models: Vec::new(),
        quota_left: vec![QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(127),
            lasts_to_reset: false,
        }],
        unreadable_lines: 2,
    }
}

fn flow_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join(TOKENS_DIR)).unwrap();
    dir
}

#[test]
fn test_token_flow_round_trips() {
    let flow = sample_flow();
    let bytes = serde_json::to_vec(&flow).unwrap();
    assert_eq!(serde_json::from_slice::<TokenFlow>(&bytes).unwrap(), flow);
}

#[test]
fn test_kind_serializes_wire_names() {
    assert_eq!(
        serde_json::to_value([AgentKind::Main, AgentKind::Subagent, AgentKind::Codex]).unwrap(),
        serde_json::json!(["main", "subagent", "codex"])
    );
}

#[test]
fn test_provider_serializes_wire_names() {
    assert_eq!(
        serde_json::to_value([QuotaProvider::Claude, QuotaProvider::Codex]).unwrap(),
        serde_json::json!(["claude", "codex"])
    );
}

#[test]
fn test_none_agent_type_serializes_present_null() {
    let value = serde_json::to_value(sample_flow()).unwrap();
    assert_eq!(
        value["stages"][0]["agents"][0].get("agent_type"),
        Some(&serde_json::Value::Null)
    );
}

#[test]
fn test_read_flow_missing_dir_returns_none() {
    let dir = TempDir::new().unwrap();
    assert_eq!(read_flow(&dir.path().join("missing")), None);
}

#[test]
fn test_read_flow_written_file_returns_flow() {
    let dir = flow_dir();
    let flow = sample_flow();
    std::fs::write(flow_path(dir.path()), serde_json::to_vec(&flow).unwrap()).unwrap();

    assert_eq!(read_flow(dir.path()), Some(flow));
}

#[cfg(unix)]
#[test]
fn test_read_flow_symlink_returns_none() {
    let dir = flow_dir();
    let target = dir.path().join("target.json");
    std::fs::write(&target, serde_json::to_vec(&sample_flow()).unwrap()).unwrap();
    std::os::unix::fs::symlink(target, flow_path(dir.path())).unwrap();

    assert_eq!(read_flow(dir.path()), None);
}

#[test]
fn test_read_flow_oversized_file_returns_none() {
    let dir = flow_dir();
    let mut bytes = serde_json::to_vec(&sample_flow()).unwrap();
    bytes.resize(MAX_FLOW_BYTES as usize + 1, b' ');
    std::fs::write(flow_path(dir.path()), bytes).unwrap();

    assert_eq!(read_flow(dir.path()), None);
}

#[test]
fn test_read_flow_directory_returns_none() {
    let dir = flow_dir();
    std::fs::create_dir(flow_path(dir.path())).unwrap();

    assert_eq!(read_flow(dir.path()), None);
}

#[test]
fn test_read_flow_invalid_json_returns_none() {
    let dir = flow_dir();
    std::fs::write(flow_path(dir.path()), b"{invalid").unwrap();

    assert_eq!(read_flow(dir.path()), None);
}

#[test]
fn test_fresh_excludes_cache_reads() {
    assert_eq!(sample_flow().plan.total.fresh(), 70);
}

#[test]
fn test_fresh_saturates() {
    let usage = TokenUsage {
        input: u64::MAX - 1,
        cache_write: 2,
        cache_read: 0,
        output: 1,
    };
    assert_eq!(usage.fresh(), u64::MAX);
}

#[test]
fn test_add_saturates_each_field() {
    let mut usage = TokenUsage {
        input: u64::MAX - 1,
        cache_write: u64::MAX - 2,
        cache_read: u64::MAX - 3,
        output: u64::MAX - 4,
    };
    usage.add(&sample_flow().plan.total);

    assert_eq!(
        usage,
        TokenUsage {
            input: u64::MAX,
            cache_write: u64::MAX,
            cache_read: u64::MAX,
            output: u64::MAX,
        }
    );
}

#[test]
fn test_add_sums_corresponding_fields() {
    let mut usage = TokenUsage {
        input: 1,
        cache_write: 2,
        cache_read: 3,
        output: 4,
    };
    usage.add(&sample_flow().plan.total);

    assert_eq!(
        usage,
        TokenUsage {
            input: 11,
            cache_write: 22,
            cache_read: 33,
            output: 44,
        }
    );
}

#[cfg(unix)]
#[test]
fn test_read_flow_symlinked_tokens_directory_returns_none() {
    let work = TempDir::new().unwrap();
    let real = work.path().join("elsewhere");
    std::fs::create_dir(&real).unwrap();
    std::fs::write(
        real.join(FLOW_FILE),
        serde_json::to_vec(&sample_flow()).unwrap(),
    )
    .unwrap();
    std::os::unix::fs::symlink(&real, work.path().join(TOKENS_DIR)).unwrap();

    assert_eq!(read_flow(work.path()), None);
}

#[cfg(unix)]
#[test]
fn test_read_flow_fifo_returns_none_without_blocking() {
    let dir = flow_dir();
    nix::unistd::mkfifo(&flow_path(dir.path()), nix::sys::stat::Mode::S_IRWXU).unwrap();

    assert_eq!(read_flow(dir.path()), None);
}
