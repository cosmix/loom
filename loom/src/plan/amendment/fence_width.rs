use crate::plan::parser::closing_run;

pub(super) fn widen_fences(
    prefix: &str,
    fence_start: usize,
    fence_len: usize,
    suffix: &str,
    wanted: usize,
) -> (String, String) {
    let fence = "`".repeat(wanted);
    let prefix = format!(
        "{}{fence}{}",
        &prefix[..fence_start],
        &prefix[fence_start + fence_len..]
    );

    // The suffix starts at the closing fence line; widen its run to match.
    let closing_line = suffix.split('\n').next().unwrap_or(suffix);
    let suffix = match closing_run(closing_line) {
        Some(run) if run < wanted => {
            let indent = closing_line.len() - closing_line.trim_start_matches(' ').len();
            format!("{}{fence}{}", &suffix[..indent], &suffix[indent + run..])
        }
        _ => suffix.to_string(),
    };
    (prefix, suffix)
}
