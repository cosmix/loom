use colored::Colorize;

pub fn format_u64(value: u64) -> String {
    grouped(&value.to_string())
}

pub fn format_f64(value: f64) -> String {
    let rendered = format!("{value:.1}");
    match rendered.split_once('.') {
        Some((whole, fraction)) => format!("{}.{}", grouped(whole), fraction),
        None => grouped(&rendered),
    }
}

pub fn row(label: &str, value: impl std::fmt::Display) {
    println!("  {label}: {value}");
}

pub fn heading(label: &str) {
    println!("\n{}", label.bold().cyan());
}

pub fn no_data(label: &str) {
    println!("  {label}: no data");
}

/// Nearest-rank percentile: sorts `values` in place and returns the element at
/// `ceil(len * fraction) - 1`. `0` (via `T::default()`) for an empty slice.
pub(super) fn percentile<T: Copy + Ord + Default>(values: &mut [T], fraction: f64) -> T {
    if values.is_empty() {
        return T::default();
    }
    values.sort_unstable();
    let index = ((values.len() as f64 * fraction).ceil() as usize).saturating_sub(1);
    values[index]
}

/// `value` as a percentage of `total`, `0.0` when `total` is zero.
pub(super) fn share(value: f64, total: f64) -> f64 {
    if total == 0.0 {
        0.0
    } else {
        value * 100.0 / total
    }
}

fn grouped(raw: &str) -> String {
    let (sign, digits) = raw.strip_prefix('-').map_or(("", raw), |rest| ("-", rest));
    let mut output = String::with_capacity(raw.len() + raw.len() / 3);
    output.push_str(sign);
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            output.push(',');
        }
        output.push(character);
    }
    output
}
