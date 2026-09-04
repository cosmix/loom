use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::commands::status::ui::theme::{StatusColors, Theme};

use super::text::padded;
use super::{Column, ColumnKind, FULL_WIDTH, MIN_COLS};

const DESIGNED_COLUMNS: &[(ColumnKind, u16)] = &[
    (ColumnKind::State, 12),
    (ColumnKind::Stage, 25),
    (ColumnKind::DependsOn, 16),
    (ColumnKind::Models, 16),
    (ColumnKind::Activity, 14),
    (ColumnKind::Context, 9),
    (ColumnKind::Time, 7),
    (ColumnKind::Merge, 8),
];

/// Return the ledger columns that fit in `width` cells.
pub fn columns_for_width(width: u16) -> Vec<Column> {
    if width < MIN_COLS {
        return Vec::new();
    }

    let mut cols: Vec<Column> = DESIGNED_COLUMNS
        .iter()
        .map(|&(kind, width)| Column { kind, width })
        .collect();
    for kind in [
        ColumnKind::Time,
        ColumnKind::Models,
        ColumnKind::DependsOn,
        ColumnKind::Context,
    ] {
        if total(&cols) <= width {
            break;
        }
        cols.retain(|column| column.kind != kind);
    }
    expand_columns(&mut cols, width);
    cols
}

/// Return the padded ledger column headings.
pub fn header_line(cols: &[Column]) -> Line<'static> {
    let mut spans = Vec::new();
    for column in cols {
        spans.push(Span::raw(" ".repeat(usize::from(gap_before(column.kind)))));
        spans.push(Span::styled(
            padded(header(column.kind), column.width),
            Theme::dimmed(),
        ));
    }
    Line::from(spans)
}

/// Return a border rule spanning `width` cells.
pub fn rule_line(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(usize::from(width)),
        Style::default().fg(StatusColors::BORDER),
    ))
}

/// Return the gap immediately before a ledger column.
pub fn gap_before(kind: ColumnKind) -> u16 {
    match kind {
        ColumnKind::State => 0,
        ColumnKind::Merge => 1,
        _ => 2,
    }
}

fn expand_columns(cols: &mut [Column], width: u16) {
    if width <= FULL_WIDTH {
        return;
    }

    let mut extra = width - FULL_WIDTH;
    let stage_extra = extra.min(15);
    add_width(cols, ColumnKind::Stage, stage_extra);
    extra -= stage_extra;

    let models_extra = extra.min(8);
    add_width(cols, ColumnKind::Models, models_extra);
    extra -= models_extra;

    add_width(cols, ColumnKind::DependsOn, extra);
}

fn add_width(cols: &mut [Column], kind: ColumnKind, extra: u16) {
    if let Some(column) = cols.iter_mut().find(|column| column.kind == kind) {
        column.width += extra;
    }
}

fn total(cols: &[Column]) -> u16 {
    cols.iter()
        .map(|column| column.width + gap_before(column.kind))
        .sum()
}

fn header(kind: ColumnKind) -> &'static str {
    match kind {
        ColumnKind::State => "STATE",
        ColumnKind::Stage => "STAGE",
        ColumnKind::DependsOn => "DEPENDS ON",
        ColumnKind::Models => "MODELS",
        ColumnKind::Activity => "ACTIVITY",
        ColumnKind::Context => "CONTEXT",
        ColumnKind::Time => "TIME",
        ColumnKind::Merge => "MERGE",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_width_has_all_columns() {
        let cols = columns_for_width(120);
        assert_eq!(cols.len(), 8);
        assert_eq!(total(&cols), 120);
    }

    #[test]
    fn one_cell_short_drops_time() {
        let cols = columns_for_width(119);
        assert!(!cols.iter().any(|column| column.kind == ColumnKind::Time));
        assert_eq!(total(&cols), 111);
    }

    #[test]
    fn narrow_width_drops_models() {
        let cols = columns_for_width(110);
        assert!(!cols.iter().any(|column| column.kind == ColumnKind::Models));
        assert_eq!(total(&cols), 93);
    }

    #[test]
    fn narrower_width_drops_dependencies() {
        let cols = columns_for_width(92);
        assert!(!cols
            .iter()
            .any(|column| column.kind == ColumnKind::DependsOn));
        assert_eq!(total(&cols), 75);
    }

    #[test]
    fn minimum_width_drops_context() {
        let cols = columns_for_width(74);
        assert!(!cols.iter().any(|column| column.kind == ColumnKind::Context));
        assert_eq!(total(&cols), 64);
    }

    #[test]
    fn below_minimum_has_no_columns() {
        assert!(columns_for_width(63).is_empty());
    }

    #[test]
    fn surplus_widens_stage_then_models() {
        let cols = columns_for_width(140);
        assert_eq!(cols[1].width, 40); // Stage: 25 + 15 (capped)
        assert_eq!(cols[3].width, 21); // Models: 16 + 5 (remaining surplus)
        assert_eq!(cols[2].width, 16); // DependsOn: no surplus left, unchanged
        assert_eq!(total(&cols), 140);
    }

    #[test]
    fn large_surplus_caps_models_then_widens_dependencies() {
        let cols = columns_for_width(160);
        assert_eq!(cols[1].width, 40); // Stage: 25 + 15 (capped)
        assert_eq!(cols[3].width, 24); // Models: 16 + 8 (capped)
        assert_eq!(cols[2].width, 33); // DependsOn: 16 + 17 (remaining surplus)
        assert_eq!(total(&cols), 160);
    }

    #[test]
    fn exact_full_width_leaves_models_at_designed_width() {
        let cols = columns_for_width(FULL_WIDTH);
        let models = cols
            .iter()
            .find(|column| column.kind == ColumnKind::Models)
            .unwrap();
        assert_eq!(models.width, 16);
    }

    #[test]
    fn headers_and_rule_fill_their_widths() {
        let cols = columns_for_width(120);
        assert_eq!(header_line(&cols).width(), 120);
        assert_eq!(rule_line(120).width(), 120);
    }
}
