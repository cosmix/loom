//! Unified diff rendering for the contract dispute briefing
//! (`prompt/contract.rs`).

/// The line spans a unified diff hunk needs: the shared prefix/suffix of
/// unchanged lines around the change, and the derived hunk bounds.
struct Bounds {
    start: usize,
    prefix: usize,
    old_change_end: usize,
    new_change_end: usize,
    old_end: usize,
    new_end: usize,
}

fn compute_bounds(old: &[&str], new: &[&str], before: &str, after: &str) -> Bounds {
    let mut prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    if old == new {
        prefix = prefix.saturating_sub(1);
    }
    let mut suffix = if old == new {
        0
    } else {
        old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count()
    };
    if before.ends_with('\n') != after.ends_with('\n') && suffix > 0 {
        suffix -= 1;
    }
    let old_change_end = old.len() - suffix;
    let new_change_end = new.len() - suffix;
    Bounds {
        start: prefix.saturating_sub(3),
        prefix,
        old_change_end,
        new_change_end,
        old_end: (old_change_end + 3).min(old.len()),
        new_end: (new_change_end + 3).min(new.len()),
    }
}

fn diff_header(file: &str, bounds: &Bounds) -> String {
    format!(
        "--- frozen/{file}\n+++ current/{file}\n@@ -{},{} +{},{} @@\n",
        if bounds.old_end == bounds.start {
            bounds.start
        } else {
            bounds.start + 1
        },
        bounds.old_end - bounds.start,
        if bounds.new_end == bounds.start {
            bounds.start
        } else {
            bounds.start + 1
        },
        bounds.new_end - bounds.start
    )
}

fn push_diff_line(out: &mut String, marker: char, line: &str, last: bool, terminated: bool) {
    out.push(marker);
    out.push_str(line);
    out.push('\n');
    if last && !terminated {
        out.push_str("\\ No newline at end of file\n");
    }
}

fn push_diff_body(
    out: &mut String,
    old: &[&str],
    new: &[&str],
    bounds: &Bounds,
    before: &str,
    after: &str,
) {
    for (i, line) in old
        .iter()
        .enumerate()
        .take(bounds.prefix)
        .skip(bounds.start)
    {
        push_diff_line(out, ' ', line, i + 1 == old.len(), before.ends_with('\n'));
    }
    for (i, line) in old
        .iter()
        .enumerate()
        .take(bounds.old_change_end)
        .skip(bounds.prefix)
    {
        push_diff_line(out, '-', line, i + 1 == old.len(), before.ends_with('\n'));
    }
    for (i, line) in new
        .iter()
        .enumerate()
        .take(bounds.new_change_end)
        .skip(bounds.prefix)
    {
        push_diff_line(out, '+', line, i + 1 == new.len(), after.ends_with('\n'));
    }
    for (i, line) in old
        .iter()
        .enumerate()
        .take(bounds.old_end)
        .skip(bounds.old_change_end)
    {
        push_diff_line(out, ' ', line, i + 1 == old.len(), before.ends_with('\n'));
    }
}

/// A unified diff of `before` vs `after`, headed `frozen/{file}` /
/// `current/{file}`, byte-identical to what `prompt/contract.rs` used to
/// build inline.
pub(super) fn unified_diff(before: &str, after: &str, file: &str) -> String {
    if before == after {
        return "(no differences)\n".to_string();
    }
    let old: Vec<&str> = before.split_terminator('\n').collect();
    let new: Vec<&str> = after.split_terminator('\n').collect();
    let bounds = compute_bounds(&old, &new, before, after);
    let mut out = diff_header(file, &bounds);
    push_diff_body(&mut out, &old, &new, &bounds, before, after);
    out
}
