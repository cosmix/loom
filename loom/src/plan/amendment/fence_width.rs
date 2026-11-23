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

    let after_indent = suffix.trim_start_matches(' ');
    let indent = suffix.len() - after_indent.len();
    let after_run = after_indent.trim_start_matches('`');
    let suffix = if after_indent.len() - after_run.len() < wanted {
        format!("{}{fence}{after_run}", &suffix[..indent])
    } else {
        suffix.to_string()
    };
    (prefix, suffix)
}
