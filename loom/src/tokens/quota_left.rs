//! Quota time-left estimate from the five-hour usage slope.

pub const SLOPE_WINDOW_SECS: i64 = 1800;
pub const MIN_SPAN_SECS: i64 = 360;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuotaEstimate {
    pub minutes_left: Option<u32>,
    pub lasts_to_reset: bool,
}

/// Points are oldest first, for one provider's five-hour window and reset period.
/// Each point contains Unix seconds and used percent; non-finite usage is ignored.
pub fn estimate(points: &[(i64, f64)], resets_at: Option<i64>, now: i64) -> QuotaEstimate {
    let unknown = QuotaEstimate {
        minutes_left: None,
        lasts_to_reset: false,
    };
    let mut recent = points
        .iter()
        .filter(|(_, used)| used.is_finite())
        .filter(|(at, _)| *at >= now.saturating_sub(SLOPE_WINDOW_SECS));
    let Some(first) = recent.next() else {
        return unknown;
    };
    let Some(last) = recent.next_back() else {
        return unknown;
    };
    let span = i128::from(last.0) - i128::from(first.0);
    if span < i128::from(MIN_SPAN_SECS) {
        return unknown;
    }
    let slope = (last.1 - first.1) / (span as f64 / 60.0);
    if slope <= 0.0 {
        return QuotaEstimate {
            minutes_left: None,
            lasts_to_reset: true,
        };
    }
    let elapsed = (i128::from(now) - i128::from(last.0)) as f64 / 60.0;
    let minutes_left = ((100.0 - last.1).max(0.0) / slope - elapsed)
        .max(0.0)
        .floor()
        .min(f64::from(u32::MAX)) as u32;
    QuotaEstimate {
        minutes_left: Some(minutes_left),
        lasts_to_reset: resets_at.is_some_and(|reset| {
            i128::from(now) + i128::from(minutes_left) * 60 >= i128::from(reset)
        }),
    }
}

#[cfg(test)]
#[path = "quota_left_tests.rs"]
mod tests;
