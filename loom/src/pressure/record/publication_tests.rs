use super::publication::{AFTER_APPEND, AFTER_COMPARISON, WRITE_LIMIT};
use super::tests::{plan_file, record_line, END};
use super::*;
use std::io::Write;
use std::os::unix::fs::symlink;

#[test]
fn exact_appendable_layouts_preserve_prose_crlf_and_existing_records() -> Result<()> {
    let line = record_line(&["new"], 1, &[]);
    for text in [
        format!("# Prose\nKeep café.\n{END}"),
        format!("{END}\n## Pressure tests\n\n- old\n"),
        format!("{END}\n## Pressure tests\n\n- old"),
        "# Prose\r\n<!-- END loom METADATA -->\r\n\r\n## Pressure tests\r\n\r\n- old\r\n".into(),
    ] {
        let (dir, path) = plan_file(&text);
        assert_eq!(append_record(&path, &line)?, RecordWrite::Written);
        assert_eq!(
            fs::read_to_string(&path)?,
            append_to_text(&text, &line.render())
        );
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    }
    Ok(())
}

#[test]
fn interior_sections_and_trailing_blanks_are_refused_before_mutation() -> Result<()> {
    let line = record_line(&["new"], 1, &[]);
    for tail in ["\n## Notes\nKeep café.\n", "\n", "\r\n\r\n"] {
        let text = format!("{END}\n## Pressure tests\n\n- old\n{tail}");
        let (dir, path) = plan_file(&text);
        let error = append_record(&path, &line).unwrap_err().to_string();
        assert!(error.contains("Cannot safely insert"), "{error}");
        assert!(error.contains(&line.render()), "{error}");
        assert_eq!(fs::read_to_string(&path)?, text);
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    }
    Ok(())
}

#[test]
fn late_in_place_edits_truncation_and_append_preserve_editor_bytes() -> Result<()> {
    for mode in ["truncate", "overwrite", "append"] {
        let (dir, path) = plan_file(END);
        AFTER_COMPARISON.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |path| match mode {
                "truncate" => fs::write(path, "New user text.\n").unwrap(),
                "overwrite" => OpenOptions::new()
                    .write(true)
                    .open(path)
                    .unwrap()
                    .write_all(b"User")
                    .unwrap(),
                _ => OpenOptions::new()
                    .append(true)
                    .open(path)
                    .unwrap()
                    .write_all(b"User appendix.\n")
                    .unwrap(),
            }));
        });
        assert_eq!(
            append_record(&path, &record_line(&["new"], 1, &[]))?,
            RecordWrite::PlanMoved
        );
        let content = fs::read_to_string(&path)?;
        let expected = match mode {
            "truncate" => "New user text.\n".to_owned(),
            "overwrite" => format!("User{}", &END[4..]),
            _ => format!("{END}User appendix.\n"),
        };
        assert!(content.starts_with(&expected), "{content}");
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    }
    Ok(())
}

#[test]
fn late_atomic_replacement_never_writes_the_new_inode() -> Result<()> {
    let (dir, path) = plan_file(END);
    AFTER_COMPARISON.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|path| {
            let replacement = path.with_extension("replacement");
            fs::write(&replacement, "New plan.\n").unwrap();
            fs::rename(replacement, path).unwrap();
        }));
    });
    assert_eq!(
        append_record(&path, &record_line(&["new"], 1, &[]))?,
        RecordWrite::PlanMoved
    );
    assert_eq!(fs::read_to_string(&path)?, "New plan.\n");
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn late_symlink_swap_never_writes_target() -> Result<()> {
    let (dir, path) = plan_file(END);
    let target = dir.path().join("target.md");
    fs::write(&target, "Target text.\n")?;
    AFTER_COMPARISON.with(|hook| {
        let target = target.clone();
        *hook.borrow_mut() = Some(Box::new(move |path| {
            fs::remove_file(path).unwrap();
            symlink(target, path).unwrap();
        }));
    });
    assert_eq!(
        append_record(&path, &record_line(&["new"], 1, &[]))?,
        RecordWrite::PlanMoved
    );
    assert_eq!(fs::read_to_string(&target)?, "Target text.\n");
    assert_eq!(fs::read_link(&path)?, target);
    assert_eq!(fs::read_dir(dir.path())?.count(), 2);
    Ok(())
}

#[test]
fn edits_through_a_hard_link_preserve_user_bytes() -> Result<()> {
    let (dir, path) = plan_file(END);
    let alias = dir.path().join("alias.md");
    fs::hard_link(&path, &alias)?;
    AFTER_COMPARISON.with(|hook| {
        let alias = alias.clone();
        *hook.borrow_mut() = Some(Box::new(move |_| {
            fs::write(alias, "Alias edit.\n").unwrap()
        }));
    });
    assert_eq!(
        append_record(&path, &record_line(&["new"], 1, &[]))?,
        RecordWrite::PlanMoved
    );
    assert!(fs::read_to_string(&path)?.starts_with("Alias edit.\n"));
    assert_eq!(fs::read(&path)?, fs::read(&alias)?);
    assert_eq!(fs::read_dir(dir.path())?.count(), 2);
    Ok(())
}

#[test]
fn replacement_after_append_is_never_reported_written() -> Result<()> {
    let (dir, path) = plan_file(END);
    AFTER_APPEND.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|path| {
            let replacement = path.with_extension("replacement");
            fs::write(&replacement, "New plan.\n").unwrap();
            fs::rename(replacement, path).unwrap();
        }));
    });
    assert_eq!(
        append_record(&path, &record_line(&["new"], 1, &[]))?,
        RecordWrite::PlanMoved
    );
    assert_eq!(fs::read_to_string(&path)?, "New plan.\n");
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn short_write_is_not_retried_or_rolled_back() -> Result<()> {
    for limit in [0, 7] {
        let (dir, path) = plan_file(END);
        WRITE_LIMIT.with(|slot| slot.set(Some(limit)));
        let line = record_line(&["new"], 1, &[]);
        let error = append_record(&path, &line).unwrap_err().to_string();
        assert!(error.contains("incomplete"), "{error}");
        assert!(error.contains("fragment may remain"), "{error}");
        let expected = append_to_text(END, &line.render());
        assert_eq!(fs::read_to_string(&path)?, &expected[..END.len() + limit]);
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    }
    Ok(())
}

#[test]
fn a_short_write_reports_the_record_and_a_retry_starts_on_its_own_line() -> Result<()> {
    // Short inside the new section's heading, and short inside the record line.
    let section = format!("{END}\n## Pressure tests\n\n- old\n");
    for (text, limit) in [(END.to_owned(), 7), (section, 7)] {
        let (dir, path) = plan_file(&text);
        let line = record_line(&["new"], 1, &[]);
        WRITE_LIMIT.with(|slot| slot.set(Some(limit)));
        let error = append_record(&path, &line).unwrap_err().to_string();
        assert!(error.contains(&line.render()), "{error}");

        let fragmented = fs::read_to_string(&path)?;
        assert!(fragmented.starts_with(&text));
        assert_eq!(fragmented.len(), text.len() + limit);
        assert_eq!(append_record(&path, &line)?, RecordWrite::Written);
        let repaired = fs::read_to_string(&path)?;
        assert!(repaired.starts_with(&fragmented), "{repaired}");
        assert!(
            repaired.lines().any(|candidate| candidate == line.render()),
            "{repaired}"
        );
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    }
    Ok(())
}

#[test]
fn oversized_result_and_special_mode_are_refused_before_mutation() -> Result<()> {
    let (dir, path) = plan_file(&"x".repeat(MAX_PLAN_BYTES as usize));
    let line = record_line(&["new"], 1, &[]);
    assert!(append_record(&path, &line)
        .unwrap_err()
        .to_string()
        .contains("exceed"));
    assert_eq!(fs::metadata(&path)?.len(), MAX_PLAN_BYTES);
    fs::write(&path, END)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o2640))?;
    assert!(append_record(&path, &line)
        .unwrap_err()
        .to_string()
        .contains("setuid/setgid"));
    assert_eq!(fs::read_to_string(&path)?, END);
    assert_eq!(fs::metadata(&path)?.mode() & 0o7777, 0o2640);
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn crash_child() {
    let Ok(path) = std::env::var("LOOM_PRESSURE_CRASH_PATH") else {
        return;
    };
    let phase = std::env::var("LOOM_PRESSURE_CRASH_PHASE").unwrap();
    if phase == "short" || phase == "full" {
        let remaining = if phase == "short" { 7 } else { 0 };
        AFTER_COMPARISON.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |_| {
                let limit = libc::rlimit {
                    rlim_cur: (END.len() + remaining) as libc::rlim_t,
                    rlim_max: (END.len() + remaining) as libc::rlim_t,
                };
                // SAFETY: this isolated child limits only itself; the signal
                // disposition allows checking the write error instead of death.
                unsafe {
                    libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                    assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
                }
            }));
        });
        let error = append_record(Path::new(&path), &record_line(&["new"], 1, &[]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("fragment may remain"), "{error}");
        std::process::exit(81);
    }
    let hook = match phase.as_str() {
        "prepared" => &BEFORE_PUBLICATION,
        "appended" => &AFTER_APPEND,
        _ => panic!("invalid crash phase"),
    };
    hook.with(|hook| *hook.borrow_mut() = Some(Box::new(|_| std::process::exit(81))));
    append_record(Path::new(&path), &record_line(&["new"], 1, &[])).unwrap();
    panic!("crash hook did not run");
}

#[test]
fn process_exit_and_real_short_writes_preserve_existing_plan() -> Result<()> {
    for phase in ["prepared", "appended", "short", "full"] {
        let (_dir, path) = plan_file(END);
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "pressure::record::publication_tests::crash_child",
                "--nocapture",
            ])
            .env("LOOM_PRESSURE_CRASH_PATH", &path)
            .env("LOOM_PRESSURE_CRASH_PHASE", phase)
            .status()?;
        assert_eq!(status.code(), Some(81));
        let expected = if phase == "prepared" || phase == "full" {
            END.to_owned()
        } else {
            let updated = append_to_text(END, &record_line(&["new"], 1, &[]).render());
            if phase == "short" {
                updated[..END.len() + 7].to_owned()
            } else {
                updated
            }
        };
        assert_eq!(fs::read_to_string(&path)?, expected);
    }
    Ok(())
}

#[test]
fn concurrent_loom_record_writers_preserve_both_records() -> anyhow::Result<()> {
    let (_dir, plan) = plan_file(END);
    let (prepared_tx, prepared_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (second_tx, second_rx) = std::sync::mpsc::channel();
    let first = plan.clone();
    let second = plan.clone();
    let one = std::thread::spawn(move || {
        BEFORE_PUBLICATION.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move |_| {
                prepared_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
            }));
        });
        append_record(&first, &record_line(&["one"], 1, &[]))
    });
    prepared_rx.recv_timeout(std::time::Duration::from_secs(5))?;
    let two = std::thread::spawn(move || {
        let result = append_record(&second, &record_line(&["two"], 1, &[]));
        second_tx.send(()).unwrap();
        result
    });
    let waiting = second_rx.recv_timeout(std::time::Duration::from_millis(100));
    release_tx.send(())?;
    assert!(matches!(
        waiting,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    ));
    assert_eq!(one.join().unwrap()?, RecordWrite::Written);
    assert_eq!(two.join().unwrap()?, RecordWrite::Written);
    let content = fs::read_to_string(&plan)?;
    assert!(content.contains("Codex one"));
    assert!(content.contains("Codex two"));
    Ok(())
}
