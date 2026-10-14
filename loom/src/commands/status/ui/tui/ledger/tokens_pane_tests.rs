//! Token pane formatting and rendering contracts.

use super::*;
use crate::tokens::{PlanFlow, QuotaProvider, TokenUsage};

fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

fn agent(id: &str, kind: AgentKind, model: &str, total: TokenUsage, rate: TokenUsage) -> AgentFlow {
    AgentFlow {
        id: id.into(),
        kind,
        agent_type: None,
        model: Some(model.into()),
        active: true,
        total,
        rate,
        series: Vec::new(),
    }
}

fn fixture_agents() -> Vec<AgentFlow> {
    let mut main = agent(
        "main-01ed9f4c",
        AgentKind::Main,
        "claude-opus-5-5",
        usage(40_000, 600_000, 9_000_000, 100_000),
        usage(600, 12_000, 200_000, 1_600),
    );
    main.series = vec![7_100; 120];
    let mut subagent = agent(
        "a04baa183f888b773",
        AgentKind::Subagent,
        "claude-sonnet-5-5",
        usage(50_000, 900_000, 13_000_000, 160_000),
        usage(1_200, 21_000, 350_000, 3_000),
    );
    subagent.agent_type = Some("loom-software-engineer".into());
    subagent.series = vec![12_600; 120];
    let mut codex = agent(
        "codex-01a1396b",
        AgentKind::Codex,
        "gpt-6.1-sol",
        usage(30_000, 300_000, 4_000_000, 50_000),
        usage(200, 3_000, 50_000, 600),
    );
    codex.series = vec![1_900; 120];
    vec![main, subagent, codex]
}

fn fixture() -> TokenFlow {
    let total = usage(120_000, 1_800_000, 26_000_000, 310_000);
    let rate = usage(2_000, 36_000, 600_000, 5_200);
    TokenFlow {
        observed_at: 1_788_523_200,
        bucket_secs: 30,
        plan: PlanFlow {
            total,
            rate,
            series: vec![21_600; 60],
        },
        stages: vec![StageFlow {
            stage_id: "server".into(),
            total,
            rate,
            series: vec![21_600; 120],
            agents: fixture_agents(),
        }],
        quota_left: vec![QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(127),
            lasts_to_reset: false,
        }],
        unreadable_lines: 0,
    }
}

#[test]
fn test_compact_tokens_boundaries_truncate() {
    for (n, expected) in [
        (0, "0"),
        (999, "999"),
        (1_000, "1.0k"),
        (21_800, "21.8k"),
        (99_999, "99.9k"),
        (312_000, "312k"),
        (999_999, "999k"),
        (2_230_000, "2.23M"),
        (u64::MAX, "18446744073709.55M"),
    ] {
        assert_eq!(compact_tokens(n), expected);
    }
}

#[test]
fn test_spark_pads_and_scales_only_visible_values() {
    for (values, cells, expected) in [
        (vec![], 3, "   "),
        (vec![0, 0], 4, "    "),
        (vec![0, 1, 8], 5, "   ▁█"),
        (vec![100, 0, 1, 8], 3, " ▁█"),
        (vec![u64::MAX], 1, "█"),
        (vec![1], 0, ""),
    ] {
        let result = spark(&values, cells);
        assert_eq!((result.as_str(), result.chars().count()), (expected, cells));
    }
}

#[test]
fn test_shade_strip_zero_and_extremes_are_safe() {
    assert_eq!(shade_strip(&[0, 1, 2, 3, 4, 5], 4), " ░▒▓██");
    assert_eq!(shade_strip(&[0, u64::MAX], u64::MAX), " █");
    assert_eq!(shade_strip(&[0, 1], 0), "  ");
}

#[test]
fn test_quota_left_text_duration_and_tone() {
    for (minutes, text, color) in [
        (127, "~2h07m left", Color::Yellow),
        (43, "~43m left", Color::Yellow),
        (30, "~30m left", Color::Yellow),
        (20, "~20m left", Color::Red),
    ] {
        let left = QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(minutes),
            lasts_to_reset: false,
        };
        assert_eq!(
            quota_left_text(&left),
            Some((text.into(), Style::default().fg(color)))
        );
    }
}

#[test]
fn test_quota_left_text_reset_precedes_duration_and_unknown_is_absent() {
    let mut left = QuotaLeft {
        provider: QuotaProvider::Claude,
        minutes_left: Some(20),
        lasts_to_reset: true,
    };
    assert_eq!(
        quota_left_text(&left),
        Some(("lasts to reset".into(), Theme::dimmed()))
    );
    left.minutes_left = None;
    left.lasts_to_reset = false;
    assert_eq!(quota_left_text(&left), None);
}

#[test]
fn test_pane_fixture_renders_title_stage_and_three_agents() {
    let lines = tokens_pane_lines(&fixture(), 132, PANE_MAX_ROWS);
    let text: Vec<_> = lines.iter().map(ToString::to_string).collect();

    assert_eq!(lines.len(), 5);
    assert!(text[0].starts_with(" TOKENS · fresh per 30 s cell · ░▒▓█ low to high"));
    assert!(text[0].ends_with("t close"));
    assert!(text[1].contains("server"));
    assert!(text[1].contains("43.2k/m"));
    for (row, label, total) in [
        (2, "main", "740k"),
        (3, "software-engineer a04b", "1.11M"),
        (4, "codex 01a1", "380k"),
    ] {
        assert!(text[row].starts_with(&format!("  {label}")));
        assert!(text[row].contains("live"));
        assert!(text[row].ends_with(total));
        assert_eq!(lines[row].width(), 132);
    }
    assert!(lines.iter().all(|line| line.width() <= 132));
}

#[test]
fn test_pane_row_limits_are_respected() {
    let flow = fixture();
    assert_eq!(tokens_pane_lines(&flow, 132, 2).len(), 2);
    assert!(tokens_pane_lines(&flow, 132, 0).is_empty());
}

#[test]
fn test_pane_narrow_and_unicode_lines_fit() {
    let mut flow = fixture();
    flow.stages[0].stage_id = "界".repeat(80);
    flow.stages[0].agents[1].agent_type = Some("loom-界".repeat(80));
    flow.stages[0].agents[2].id = "codex-é界ab".into();
    for width in [0, 1, 7, 64] {
        let lines = tokens_pane_lines(&flow, width, PANE_MAX_ROWS);
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().all(|line| line.width() <= usize::from(width)));
    }
}

#[test]
fn test_pane_idle_agent_keeps_empty_strip_and_total() {
    let mut flow = fixture();
    flow.stages[0].agents[0].active = false;
    flow.stages[0].agents[0].series.clear();

    let lines = tokens_pane_lines(&flow, 132, PANE_MAX_ROWS);
    let main = lines[2].to_string();
    assert!(main.starts_with("  main"));
    assert!(main.contains(&format!("idle {}", " ".repeat(48))));
    assert!(main.ends_with("740k"));
}

#[test]
fn test_pane_stage_visibility_uses_active_flag() {
    let mut flow = fixture();
    for agent in &mut flow.stages[0].agents {
        agent.active = false;
    }
    assert_eq!(tokens_pane_lines(&flow, 132, PANE_MAX_ROWS).len(), 1);
    flow.stages[0].agents[0].active = true;
    flow.stages[0].agents[0].series.clear();
    assert_eq!(tokens_pane_lines(&flow, 132, PANE_MAX_ROWS).len(), 5);
}

/// `len` empty buckets whose newest holds `newest`.
fn newest_only(len: usize, newest: u64) -> Vec<u64> {
    let mut series = vec![0; len - 1];
    series.push(newest);
    series
}

#[test]
fn test_shrink_takes_columns_in_order_down_to_their_minimum() {
    for (room, expected) in [
        (100, [40, 30]),
        (60, [30, 30]),
        (35, [10, 25]),
        (0, [10, 10]),
    ] {
        assert_eq!(shrink(room, [(40, 10), (30, 10)]), expected);
    }
}

#[test]
fn test_columns_shrink_the_model_then_the_strip_then_the_label() {
    for (width, stage, agent) in [
        (132, (30, 48), (30, 24, 48)),
        (100, (30, 48), (30, 8, 47)),
        (80, (30, 39), (30, 8, 27)),
        (64, (30, 23), (29, 8, 12)),
    ] {
        let columns = Columns::fit(width, SLOT_MIN, SLOT_MIN);
        assert_eq!((columns.stage_id, columns.spark), stage, "stage at {width}");
        assert_eq!(
            (columns.label, columns.model, columns.strip),
            agent,
            "agent at {width}"
        );
    }
}

#[test]
fn test_pane_keeps_the_newest_cells_and_the_state_down_to_64_columns() {
    let mut flow = fixture();
    flow.stages[0].series = newest_only(120, 10);
    for agent in &mut flow.stages[0].agents {
        agent.series = newest_only(120, 10);
    }
    flow.stages[0].agents[0].active = false;

    for width in [64, 80, 100, 132] {
        let lines = tokens_pane_lines(&flow, width, PANE_MAX_ROWS);
        let text: Vec<_> = lines.iter().map(ToString::to_string).collect();

        assert_eq!(lines.len(), 5);
        assert!(text[1].ends_with('█'), "stage at {width}: {:?}", text[1]);
        for (row, total, state) in [
            (2, "740k", "idle "),
            (3, "1.11M", "live "),
            (4, "380k", "live "),
        ] {
            let body = text[row]
                .strip_suffix(total)
                .unwrap_or_else(|| panic!("no {total} at {width}: {:?}", text[row]));
            assert!(
                body.trim_end().ends_with('█'),
                "newest cell lost at {width}: {:?}",
                text[row]
            );
            assert!(text[row].contains(state), "{state:?} lost at {width}");
            assert_eq!(lines[row].width(), usize::from(width));
        }
    }
}

#[test]
fn test_pane_sizes_the_total_and_rate_slots_to_the_widest_number() {
    for (fresh, cells) in [(12_340_000, 6), (123_400_000, 7)] {
        let wide = compact_tokens(fresh);
        assert_eq!(wide.len(), cells);
        let mut flow = fixture();
        flow.stages[0].series = newest_only(120, 10);
        flow.stages[0].rate = usage(fresh, 0, 0, 0);
        for agent in &mut flow.stages[0].agents {
            agent.series = newest_only(120, 10);
        }
        flow.stages[0].agents[0].total = usage(fresh, 0, 0, 0);
        flow.stages[0].agents[0].active = false;

        for width in [64, 80, 100] {
            let lines = tokens_pane_lines(&flow, width, PANE_MAX_ROWS);
            let text: Vec<_> = lines.iter().map(ToString::to_string).collect();

            assert_eq!(lines.len(), 5);
            assert!(lines.iter().all(|line| line.width() <= usize::from(width)));
            assert!(text[1].ends_with('█'), "stage at {width}: {:?}", text[1]);
            assert!(text[1].contains(&format!("{wide}/m")), "{:?}", text[1]);
            for (row, total, state) in [
                (2, wide.as_str(), "idle "),
                (3, "1.11M", "live "),
                (4, "380k", "live "),
            ] {
                let body = text[row]
                    .strip_suffix(total)
                    .unwrap_or_else(|| panic!("no {total} at {width}: {:?}", text[row]));
                assert!(
                    body.trim_end().ends_with('█'),
                    "newest cell lost at {width} with {wide}: {:?}",
                    text[row]
                );
                assert!(text[row].contains(state), "{state:?} lost at {width}");
            }
        }
    }
}

#[test]
fn test_pane_without_active_agents_or_data_says_so() {
    let mut flow = fixture();
    for agent in &mut flow.stages[0].agents {
        agent.active = false;
    }
    let lines = tokens_pane_lines(&flow, 132, PANE_MAX_ROWS);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].to_string().ends_with("t close"));
    let note = lines[0]
        .spans
        .iter()
        .find(|span| span.content.contains("no active agent"));
    assert_eq!(note.map(|span| span.style), Some(Theme::dimmed()));
    for width in [64, 70] {
        let line = tokens_pane_lines(&flow, width, PANE_MAX_ROWS).remove(0);
        let text = line.to_string();
        assert!(text.contains("no active agent"), "{text:?} at {width}");
        assert!(text.ends_with("t close"), "{text:?} at {width}");
        assert_eq!(line.width(), usize::from(width));
    }
    assert_eq!(tokens_pane_lines(&flow, 132, 1).len(), 1);
    assert!(tokens_pane_lines(&flow, 40, PANE_MAX_ROWS)
        .iter()
        .all(|line| line.width() <= 40));

    let lines = no_data_lines(132, PANE_MAX_ROWS);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].to_string().ends_with("t close"));
    assert_eq!(lines[1].to_string(), "  no token data yet");
    assert!(no_data_lines(132, 0).is_empty());
    assert_eq!(no_data_lines(132, 1).len(), 1);
    assert!(no_data_lines(5, PANE_MAX_ROWS)
        .iter()
        .all(|line| line.width() <= 5));
}
