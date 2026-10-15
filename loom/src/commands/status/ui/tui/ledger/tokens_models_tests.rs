//! By-model block contracts.

use super::*;
use crate::tokens::TokenUsage;

fn entry(model: &str, effort: Option<&str>, usage: [u64; 4]) -> ModelUsage {
    ModelUsage {
        model: model.into(),
        effort: effort.map(str::to_owned),
        total: TokenUsage {
            input: usage[0],
            cache_write: usage[1],
            cache_read: usage[2],
            output: usage[3],
        },
    }
}

fn four() -> Vec<ModelUsage> {
    vec![
        entry(
            "claude-sonnet-5-5",
            Some("medium"),
            [50_000, 900_000, 13_000_000, 160_000],
        ),
        entry(
            "claude-opus-5-5",
            Some("xhigh"),
            [30_000, 450_000, 7_000_000, 80_000],
        ),
        entry(
            "gpt-6.1-sol",
            Some("xhigh"),
            [30_000, 300_000, 4_000_000, 50_000],
        ),
        entry(
            "claude-opus-5-5",
            Some("high"),
            [10_000, 150_000, 2_000_000, 20_000],
        ),
    ]
}

fn text(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

fn texts(models: &[ModelUsage], width: u16) -> Vec<String> {
    model_lines(models, width).iter().map(text).collect()
}

#[test]
fn test_title_and_model_rows_in_share_order() {
    let rows = texts(&four(), 132);
    assert!(rows[0].contains("BY MODEL"), "{rows:#?}");
    for needle in ["claude-sonnet-5-5", "50%", "1.11M"] {
        assert!(rows[1].contains(needle), "{needle}: {rows:#?}");
    }
    for needle in ["claude-opus-5-5", "33%", "740k"] {
        assert!(rows[3].contains(needle), "{needle}: {rows:#?}");
    }
}

#[test]
fn test_effort_rows_follow_their_model() {
    let rows = texts(&four(), 132);
    for needle in ["xhigh", "560k", "7.00M"] {
        assert!(rows[4].contains(needle), "{needle}: {rows:#?}");
    }
    for needle in ["high", "180k", "2.00M"] {
        assert!(rows[5].contains(needle), "{needle}: {rows:#?}");
    }
    assert!(rows[6].contains("gpt-6.1-sol") && rows[6].contains("17%"));
}

#[test]
fn test_empty_or_zero_input_gives_no_lines() {
    assert!(model_lines(&[], 132).is_empty());
    assert!(model_lines(&[entry("m", None, [0, 0, 0, 0])], 132).is_empty());
}

#[test]
fn test_narrow_width_drops_the_bar() {
    let rows = texts(&four(), 50);
    assert!(rows.iter().all(|row| !row.contains('█')), "{rows:#?}");
    assert!(rows[1].contains("50%"));
    assert!(texts(&four(), 80)[1].contains('█'));
}

#[test]
fn test_no_line_exceeds_the_width() {
    for width in [50, 80, 132] {
        for row in texts(&four(), width) {
            assert!(row.chars().count() <= usize::from(width), "{width}: {row}");
        }
    }
}

#[test]
fn test_missing_effort_shows_a_dash() {
    let rows = texts(&[entry("other", None, [1, 2, 3, 4])], 132);
    assert!(rows[2].contains('—'), "{rows:#?}");
}

#[test]
fn test_rows_are_cut_to_the_block_maximum() {
    let many: Vec<ModelUsage> = (0..8)
        .map(|n| entry(&format!("model-{n}"), Some("high"), [0, 0, 0, 1_000 - n]))
        .collect();
    assert_eq!(model_lines(&many, 132).len(), MODEL_BLOCK_MAX_ROWS);
}

#[test]
fn test_long_name_drops_the_bar_at_sixty_columns() {
    let name = "abcdefghijklmnopqrstuvwx";
    let models = [entry(name, Some("high"), [500_000, 0, 9_000_000, 0])];
    let rows = texts(&models, 60);
    assert!(
        rows[1].contains(name) && rows[1].contains("500k"),
        "{rows:#?}"
    );
    assert!(rows.iter().all(|row| !row.contains('█')), "{rows:#?}");
    assert!(
        rows.iter().all(|row| row.chars().count() <= 60),
        "{rows:#?}"
    );
}

#[test]
fn test_long_names_are_truncated_to_the_name_column() {
    let long = "m".repeat(40);
    let rows = texts(&[entry(&long, Some("low"), [0, 0, 0, 100])], 132);
    assert!(
        rows[1].contains('…') && !rows[1].contains(&long),
        "{rows:#?}"
    );
}
