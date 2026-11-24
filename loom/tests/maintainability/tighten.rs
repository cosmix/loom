use std::collections::BTreeMap;

use super::baseline::{self, Violation};

/// `source` with every stale row removed and every shrunk row lowered to its
/// measured size. Keeps the other lines as written. Never adds a row or raises
/// a value. Returns the new text and the number of rows changed.
pub fn tighten(source: &str, violations: &[Violation]) -> (String, usize) {
    let current = violations
        .iter()
        .map(|violation| (&violation.key, violation.measured))
        .collect::<BTreeMap<_, _>>();
    let mut tightened = String::with_capacity(source.len());
    let mut changed = 0;
    let mut ending = "";
    for (index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.lines().next().unwrap_or_default();
        let replacement = if let Ok((key, recorded)) = baseline::parse_line(line, index + 1) {
            match current.get(&key) {
                None => {
                    changed += 1;
                    continue;
                }
                Some(&measured) if measured < recorded => {
                    changed += 1;
                    Some(baseline::render_entry(&key, measured))
                }
                Some(_) => None,
            }
        } else {
            None
        };
        // Defer the terminator in case the original last line had none and is removed.
        tightened.push_str(ending);
        tightened.push_str(replacement.as_deref().unwrap_or(line));
        ending = &raw_line[line.len()..];
    }
    if source.ends_with('\n') {
        tightened.push_str(ending);
    }
    (tightened, changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline::BaselineKey;

    fn file_violation(path: &str, measured: usize) -> Violation {
        Violation {
            key: BaselineKey::File {
                path: path.to_string(),
            },
            measured,
            location: path.to_string(),
        }
    }

    #[test]
    fn removes_stale_file_and_function_rows() {
        let source = concat!(
            "# ledger\n",
            "file src/stale.rs 401\n",
            "file src/kept.rs 402\n",
            "\n",
            "function src/lib.rs stale 51\n",
        );
        let violations = [file_violation("src/kept.rs", 402)];

        let (tightened, changed) = tighten(source, &violations);

        assert_eq!(tightened, "# ledger\nfile src/kept.rs 402\n\n");
        assert_eq!(changed, 2);
    }

    #[test]
    fn lowers_shrunk_row_and_keeps_its_line_ending() {
        let source = "  # ledger\r\n  file  src/large.rs  410\r\n";
        let violations = [file_violation("src/large.rs", 405)];

        let (tightened, changed) = tighten(source, &violations);

        assert_eq!(tightened, "  # ledger\r\nfile src/large.rs 405\r\n");
        assert_eq!(changed, 1);
    }

    #[test]
    fn preserves_grown_and_exact_rows_verbatim() {
        let source = " file src/grown.rs 410 \nfile\tsrc/exact.rs\t402";
        let violations = [
            file_violation("src/grown.rs", 411),
            file_violation("src/exact.rs", 402),
        ];

        let (tightened, changed) = tighten(source, &violations);

        assert_eq!(tightened, source);
        assert_eq!(changed, 0);
    }

    #[test]
    fn leaves_unrecorded_violations_unrecorded() {
        let source = "# ledger\n";
        let violations = [file_violation("src/new.rs", 401)];

        let (tightened, changed) = tighten(source, &violations);

        assert_eq!(tightened, source);
        assert_eq!(changed, 0);
    }

    #[test]
    fn preserves_text_that_has_no_rows_to_change() {
        let source = "  # ledger\r\n \t\r\nmalformed row";
        let (tightened, changed) = tighten(source, &[]);

        assert_eq!(tightened, source);
        assert_eq!(changed, 0);
        assert!(baseline::parse(&tightened).is_err());
    }

    #[test]
    fn preserves_absence_of_trailing_newline_when_removing_last_row() {
        let source = "# ledger\r\nfile src/stale.rs 401";
        let (tightened, changed) = tighten(source, &[]);

        assert_eq!(tightened, "# ledger");
        assert_eq!(changed, 1);
    }
}
