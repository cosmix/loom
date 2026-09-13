use super::*;
use crate::commands::usage::transcript::{Entry, Request, Scope};

fn request(model: &str, resident_input: u64) -> Request {
    Request {
        message_id: None,
        timestamp: chrono::Utc::now(),
        model: model.to_owned(),
        usage: TokenUsage {
            input: resident_input,
            ..TokenUsage::default()
        },
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
        normalization: Default::default(),
    }
}

#[test]
fn windows_exclude_synthetic_requests_just_like_totals() {
    let transcript = Transcript {
        path: std::path::PathBuf::from("test.jsonl"),
        scope: Scope::Main,
        project_slug: "project".to_owned(),
        project_path: None,
        session_id: "session-1".to_owned(),
        agent_id: None,
        agent_type: None,
        stage_id: None,
        loom_session_id: None,
        first_user_entry: None,
        entries: vec![
            Entry::Assistant(Box::new(request("claude-sonnet-5", 100))),
            Entry::Assistant(Box::new(request(SYNTHETIC_MODEL, 999))),
        ],
        diagnostics: Default::default(),
    };

    let report = build(&[transcript], Windowing::FiveHour);
    assert_eq!(report.totals.requests, 1);
    assert_eq!(report.totals.fresh_input, 100);
    assert_eq!(report.windows.len(), 1);
    assert_eq!(report.windows[0].requests, 1);
}
