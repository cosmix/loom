//! The ledger's row budget and the areas cut from it: header, alerts, stage table,
//! the optional tokens pane, needs-attention panel, activity panel and footer.

use ratatui::layout::Rect;

pub(super) const HEADER_HEIGHT: u16 = 4;
const ALERT_MAX_HEIGHT: u16 = 4;
pub(super) const TABLE_MIN_HEIGHT: u16 = 6;
/// The needs-attention panel's title row and the 13 lines of the largest single
/// entry: a stage its agent blocked, with a header, three note lines, the retry
/// and when to run it, three alternatives of two lines each, and the pointer to
/// the acceptance criteria.
pub(super) const ATTENTION_MAX_HEIGHT: u16 = 14;
/// Rows the panel keeps when the table needs the rest: its title, two entry lines
/// and the "… N more lines" marker `fit_to_rows` adds.
const ATTENTION_MIN_HEIGHT: u16 = 4;

/// The footer is the legend strip alone, or the quota line above it.
fn footer_height(has_quota: bool) -> u16 {
    if has_quota {
        2
    } else {
        1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Budget {
    pub(super) alerts: u16,
    pub(super) table: u16,
    pub(super) tokens: u16,
    pub(super) attention: u16,
    pub(super) activity: u16,
    pub(super) footer: u16,
    pub(super) compact: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Areas {
    pub(super) header: Rect,
    pub(super) alerts: Rect,
    pub(super) table: Rect,
    pub(super) tokens: Rect,
    pub(super) attention: Rect,
    pub(super) activity: Rect,
    pub(super) footer: Rect,
}

/// Split `height` rows. `tokens` is the rows the tokens pane wants (0 when it is
/// hidden); the pane is the first panel dropped when the table would fall under
/// [`TABLE_MIN_HEIGHT`], then the activity panel, then attention rows.
pub(super) fn budget(
    height: u16,
    alerts: usize,
    attention_lines: usize,
    activity_len: usize,
    has_quota: bool,
    tokens: u16,
) -> Budget {
    let mut budget = Budget {
        alerts: (alerts.min(ALERT_MAX_HEIGHT as usize)) as u16,
        table: 0,
        tokens,
        attention: attention_height(attention_lines),
        activity: activity_height(activity_len),
        footer: footer_height(has_quota),
        compact: false,
    };
    budget.fit(height);

    if budget.table < TABLE_MIN_HEIGHT {
        budget.tokens = 0;
        budget.fit(height);
    }
    if budget.table < TABLE_MIN_HEIGHT {
        budget.activity = 0;
        budget.fit(height);
    }
    // Shrink the panel one row at a time, only until the table has its minimum.
    let floor = budget.attention.min(ATTENTION_MIN_HEIGHT);
    while budget.table < TABLE_MIN_HEIGHT && budget.attention > floor {
        budget.attention -= 1;
        budget.fit(height);
    }
    if budget.table < TABLE_MIN_HEIGHT {
        budget.compact = true;
        budget.fit(height);
    }
    if budget.table < TABLE_MIN_HEIGHT {
        // The quota line is the last thing to yield before the stages themselves.
        budget.footer = footer_height(false);
        budget.fit(height);
    }
    budget
}

impl Budget {
    /// Give the table whatever the other bands leave of `height`.
    fn fit(&mut self, height: u16) {
        let gap = |rows: u16| u16::from(rows > 0);
        let table_gap = u16::from(!self.compact);
        let attention_gap = if self.compact { 0 } else { gap(self.attention) };
        let reserved = HEADER_HEIGHT
            + 1
            + self.alerts
            + gap(self.alerts)
            + table_gap
            + self.tokens
            + gap(self.tokens)
            + self.attention
            + attention_gap
            + self.activity
            + gap(self.activity)
            + self.footer;
        self.table = height.saturating_sub(reserved);
    }
}

/// The panel's entry lines plus its title row, capped at [`ATTENTION_MAX_HEIGHT`].
fn attention_height(lines: usize) -> u16 {
    if lines == 0 {
        0
    } else {
        lines
            .saturating_add(1)
            .min(usize::from(ATTENTION_MAX_HEIGHT)) as u16
    }
}

fn activity_height(entries: usize) -> u16 {
    if entries == 0 {
        0
    } else {
        (1 + entries).min(6) as u16
    }
}

pub(super) fn areas(area: Rect, budget: Budget) -> Areas {
    let mut y = area.y;
    let header = take(area, &mut y, HEADER_HEIGHT);
    y += 1;
    let alerts = take(area, &mut y, budget.alerts);
    y += u16::from(budget.alerts > 0);
    let table = take(area, &mut y, budget.table);
    y += u16::from(!budget.compact);
    let tokens = take(area, &mut y, budget.tokens);
    y += u16::from(budget.tokens > 0);
    let attention = take(area, &mut y, budget.attention);
    y += u16::from(budget.attention > 0 && !budget.compact);
    let activity = take(area, &mut y, budget.activity);
    y += u16::from(budget.activity > 0);
    let footer = take(area, &mut y, budget.footer);

    Areas {
        header,
        alerts,
        table,
        tokens,
        attention,
        activity,
        footer,
    }
}

fn take(area: Rect, y: &mut u16, height: u16) -> Rect {
    let rect = Rect::new(area.x, *y, area.width, height);
    *y = y.saturating_add(height);
    rect
}
