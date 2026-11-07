use super::ratchet_loosens;

fn loosens(base: &str, head: &str) -> bool {
    ratchet_loosens(Some(base.as_bytes()), Some(head.as_bytes()))
}

#[test]
fn lower_and_removed_entries_tighten() {
    let base = "file src/a.rs 420\nfile src/b.rs 500\n";
    assert!(!loosens(base, "file src/a.rs 419\nfile src/b.rs 500\n"));
    assert!(!loosens(base, "file src/a.rs 420\n"));
    assert!(!loosens(base, "file src/a.rs 419\n"));
}

#[test]
fn higher_and_new_entries_loosen() {
    let base = "file src/a.rs 420\nfunction src/a.rs old 50\n";
    assert!(loosens(
        base,
        "file src/a.rs 421\nfunction src/a.rs old 50\n"
    ));
    assert!(loosens(
        base,
        "file src/a.rs 420\nfunction src/a.rs old 50\nfile src/b.rs 1\n"
    ));
    assert!(loosens(
        base,
        "file src/a.rs 420\nfunction src/a.rs new 50\n"
    ));
}

#[test]
fn comments_blank_lines_and_key_whitespace_do_not_loosen() {
    let base = "# baseline\nfile  src/a.rs   420\n\nfunction\tsrc/a.rs  name 50\n";
    let head = "\n # changed\nfile src/a.rs 420\nfunction src/a.rs name 50\n# end\n";
    assert!(!loosens(base, head));
}

#[test]
fn any_unparsable_line_raises_on_either_side() {
    let valid = b"file src/a.rs 420\n";
    let invalid: &[&[u8]] = &[
        b"threshold: 80%\n",
        b"max 1.5\n",
        b"7\n",
        b"file src/a.rs 420\nfile  src/a.rs 421\n",
        b"file src/a.rs 420\nissue path detail\n",
        b"max +1\n",
        b"max -1\n",
        b"max 18446744073709551616\n",
        b"max \xff\n",
    ];
    for bad in invalid {
        assert!(ratchet_loosens(Some(bad), Some(valid)), "bad base: {bad:?}");
        assert!(ratchet_loosens(Some(valid), Some(bad)), "bad head: {bad:?}");
    }
}

#[test]
fn absent_files_raise() {
    let valid = Some(b"file src/a.rs 420\n".as_slice());
    assert!(ratchet_loosens(None, valid));
    assert!(ratchet_loosens(valid, None));
    assert!(ratchet_loosens(None, None));
}

#[test]
fn real_maintainability_baseline_obeys_ceiling_direction() {
    let base = include_str!("../../../maintainability-baseline.txt");
    let line = base.lines().find(|line| line.starts_with("file ")).unwrap();
    let (key, value) = line.rsplit_once(' ').unwrap();
    let value: u64 = value.parse().unwrap();
    let lower = base.replacen(line, &format!("{key} {}", value - 1), 1);
    let higher = base.replacen(line, &format!("{key} {}", value + 1), 1);
    assert!(!loosens(base, &lower));
    assert!(loosens(base, &higher));
}
