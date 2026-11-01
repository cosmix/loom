use super::*;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};

const END: &str = "<!-- END loom METADATA -->\n";
const RENDERED: &str = "- 2026-10-29: Claude sonnet, Codex gpt-6.1-sol; 1 round, review addressed";

fn record_line(models: &[&str], rounds: u32, missing_rounds: &[u32]) -> RecordLine {
    RecordLine {
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 29).expect("valid fixed date"),
        claude_model: "sonnet".into(),
        codex_models: models.iter().map(|model| (*model).into()).collect(),
        rounds,
        missing_rounds: missing_rounds.to_vec(),
    }
}

fn assert_placement(body: &str, expected: &str) {
    assert_eq!(
        append_to_text(&format!("{END}{body}"), "- new review"),
        format!("{END}{expected}")
    );
}

fn plan_file(text: &str) -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("temporary plan directory");
    let path = dir.path().join("PLAN-review.md");
    fs::write(&path, text).expect("write plan fixture");
    (dir, path)
}

#[test]
fn placement_uses_last_end_marker() {
    let prefix = format!("{END}\n## Pressure tests\n\n- earlier block\n\n{END}");
    let text = format!("{prefix}\n## Pressure tests\n\n- latest block\n");

    assert_eq!(
        append_to_text(&text, "- new review"),
        format!("{prefix}\n## Pressure tests\n\n- latest block\n- new review\n")
    );
}

#[test]
fn placement_appends_after_items_before_notes_and_preserves_bytes() {
    assert_placement(
        "\n## Pressure tests\n\n- first\n- second  \n\n \t\n## Notes\n\nKeep café.\n",
        "\n## Pressure tests\n\n- first\n- second  \n- new review\n\n \t\n## Notes\n\nKeep café.\n",
    );
}

#[test]
fn placement_keeps_third_level_heading_inside_section() {
    assert_placement(
        "\n## Pressure tests\n\n- first\n### Details\nKept text.\n\n## Notes\nEnd.\n",
        "\n## Pressure tests\n\n- first\n### Details\nKept text.\n- new review\n\n## Notes\nEnd.\n",
    );
}

#[test]
fn placement_stops_at_top_level_heading() {
    assert_placement(
        "\n## Pressure tests\n\n- first\n\n# Appendix\nKeep.\n",
        "\n## Pressure tests\n\n- first\n- new review\n\n# Appendix\nKeep.\n",
    );
}

#[test]
fn placement_accepts_heading_trailing_whitespace() {
    assert_placement(
        "\n## Pressure tests \t\n\n- old\n",
        "\n## Pressure tests \t\n\n- old\n- new review\n",
    );
}

#[test]
fn placement_requires_exact_heading() {
    for heading in [
        "## Pressure tests extra",
        "### Pressure tests",
        " ## Pressure tests",
    ] {
        let body = format!("\n{heading}\n- other\n");
        let expected = format!("{body}\n## Pressure tests\n\n- new review\n");

        assert_placement(&body, &expected);
    }
}

#[test]
fn placement_creates_section_when_absent_after_marker() {
    assert_placement(
        "\n## Work\n\nPending",
        "\n## Work\n\nPending\n\n## Pressure tests\n\n- new review\n",
    );
}

#[test]
fn placement_without_marker_appends_even_with_existing_heading() {
    let text = "# Plan\n\n## Pressure tests\n\n- prose review\n";

    assert_eq!(
        append_to_text(text, "- new review"),
        "# Plan\n\n## Pressure tests\n\n- prose review\n\n## Pressure tests\n\n- new review\n"
    );
}

#[test]
fn placement_ignores_heading_before_marker() {
    let prefix = format!("## Pressure tests\n\n- prose review\n\n{END}");

    assert_eq!(
        append_to_text(&prefix, "- new review"),
        format!("{prefix}\n## Pressure tests\n\n- new review\n")
    );
}

#[test]
fn placement_ignores_heading_in_metadata_yaml() {
    let text = concat!(
        "<!-- loom METADATA -->\n```yaml\n",
        "description: |\n  ## Pressure tests\n  - YAML text\n",
        "```\n<!-- END loom METADATA -->\n"
    );

    assert_eq!(
        append_to_text(text, "- new review"),
        format!("{text}\n## Pressure tests\n\n- new review\n")
    );
}

#[test]
fn placement_ignores_pressure_heading_inside_fence_after_marker() {
    let text = format!("{END}\n```md\n## Pressure tests\n\n- example\n```\n");

    assert_eq!(
        append_to_text(&text, "- new review"),
        format!("{text}\n## Pressure tests\n\n- new review\n")
    );
}

#[test]
fn placement_ignores_end_marker_inside_fence() {
    let example = format!("```md\n{END}## Pressure tests\n```\n");
    assert_placement(
        &format!("\n## Pressure tests\n\n- first\n\n## Notes\n\n{example}"),
        &format!("\n## Pressure tests\n\n- first\n- new review\n\n## Notes\n\n{example}"),
    );
}

#[test]
fn placement_section_end_ignores_heading_inside_fence() {
    assert_placement(
        "\n## Pressure tests\n\n- first\n```md\n# Example\n## Other\n```\n\n## Notes\nEnd.\n",
        "\n## Pressure tests\n\n- first\n```md\n# Example\n## Other\n```\n- new review\n\n## Notes\nEnd.\n",
    );
}

#[test]
fn placement_at_eof_ends_inserted_line_with_one_newline() {
    for body in [
        "\n## Pressure tests\n\n- old",
        "\n## Pressure tests\n\n- old\n",
    ] {
        assert_placement(body, "\n## Pressure tests\n\n- old\n- new review\n");
    }
}

#[test]
fn placement_empty_input_creates_section_with_final_newline() {
    assert_eq!(
        append_to_text("", "- new review"),
        "\n\n## Pressure tests\n\n- new review\n"
    );
}

#[test]
fn placement_preserves_crlf_in_existing_section() {
    let text = concat!(
        "<!-- END loom METADATA -->\r\n",
        "\r\n## Pressure tests\r\n\r\n- old\r\n\r\n## Notes\r\nKeep.\r\n"
    );
    let expected = concat!(
        "<!-- END loom METADATA -->\r\n",
        "\r\n## Pressure tests\r\n\r\n- old\r\n- new review\r\n",
        "\r\n## Notes\r\nKeep.\r\n"
    );

    assert_eq!(append_to_text(text, "- new review"), expected);
}

#[test]
fn placement_uses_crlf_for_new_section() {
    assert_eq!(
        append_to_text("# Plan\r\nBody", "- new review"),
        "# Plan\r\nBody\r\n\r\n## Pressure tests\r\n\r\n- new review\r\n"
    );
}

#[test]
fn render_one_round_uses_singular_and_addressed_outcome() {
    assert_eq!(record_line(&["gpt-6.1-sol"], 1, &[]).render(), RENDERED);
}

#[test]
fn render_repeated_models_collapses_to_one_model() {
    assert_eq!(
        record_line(&["gpt-6.1-sol", "gpt-6.1-sol"], 2, &[]).render(),
        "- 2026-10-29: Claude sonnet, Codex gpt-6.1-sol; 2 rounds, review addressed"
    );
}

#[test]
fn render_mixed_models_preserves_every_round_in_order() {
    assert_eq!(
        record_line(&["gpt-6.1-sol", "gpt-6.1-sol", "gpt-6.1-terra"], 3, &[]).render(),
        concat!(
            "- 2026-10-29: Claude sonnet, Codex gpt-6.1-sol / gpt-6.1-sol / ",
            "gpt-6.1-terra; 3 rounds, review addressed"
        )
    );
}

#[test]
fn render_empty_model_is_unknown() {
    assert_eq!(
        record_line(&[""], 1, &[]).render(),
        "- 2026-10-29: Claude sonnet, Codex unknown; 1 round, review addressed"
    );
}

#[test]
fn render_unknown_model_keeps_its_place_among_mixed_models() {
    assert_eq!(
        record_line(&["gpt-6.1-sol", "", "gpt-6.1-terra"], 3, &[]).render(),
        concat!(
            "- 2026-10-29: Claude sonnet, Codex gpt-6.1-sol / unknown / ",
            "gpt-6.1-terra; 3 rounds, review addressed"
        )
    );
}

#[test]
fn render_several_missing_rounds_lists_round_numbers() {
    assert_eq!(
        record_line(&["gpt-6.1-sol"; 4], 4, &[1, 3, 4]).render(),
        concat!(
            "- 2026-10-29: Claude sonnet, Codex gpt-6.1-sol; 4 rounds, ",
            "Codex review missing in round 1, 3, 4"
        )
    );
}

#[cfg(unix)]
#[test]
fn append_record_keeps_mode_0640_and_writes_record() -> anyhow::Result<()> {
    let (dir, plan) = plan_file(END);
    fs::set_permissions(&plan, fs::Permissions::from_mode(0o640))?;
    let line = record_line(&["gpt-6.1-sol"], 1, &[]);

    let result = append_record(&plan, &line)?;

    assert!(matches!(result, RecordWrite::Written));
    assert_eq!(fs::metadata(&plan)?.permissions().mode() & 0o7777, 0o640);
    assert_eq!(
        fs::read_to_string(&plan)?,
        format!("{END}\n## Pressure tests\n\n{RENDERED}\n")
    );
    assert_eq!(
        fs::read_dir(dir.path())?.count(),
        1,
        "no temporary file remains"
    );
    Ok(())
}

#[test]
fn in_place_edit_after_read_skips_record_without_losing_edit() -> anyhow::Result<()> {
    let (dir, plan) = plan_file(END);
    BEFORE_PUBLICATION.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|path| {
            fs::write(path, format!("{END}An editor changed this plan.\n")).unwrap();
        }));
    });
    let result = append_record(&plan, &record_line(&["a"], 1, &[]))?;
    assert_eq!(result, RecordWrite::PlanMoved);
    assert_eq!(
        fs::read_to_string(&plan)?,
        format!("{END}An editor changed this plan.\n")
    );
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn replacement_before_publication_skips_record_without_losing_edit() -> anyhow::Result<()> {
    let (dir, plan) = plan_file(END);
    BEFORE_PUBLICATION.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|path| {
            let replacement = path.with_extension("replacement");
            fs::write(&replacement, format!("{END}A newer plan.\n")).unwrap();
            fs::rename(replacement, path).unwrap();
        }));
    });
    let result = append_record(&plan, &record_line(&["a"], 1, &[]))?;
    assert_eq!(result, RecordWrite::PlanMoved);
    assert_eq!(fs::read_to_string(&plan)?, format!("{END}A newer plan.\n"));
    assert_eq!(fs::read_dir(dir.path())?.count(), 1);
    Ok(())
}

#[test]
fn concurrent_loom_record_writers_preserve_both_records() -> anyhow::Result<()> {
    let (_dir, plan) = plan_file(END);
    let first = plan.clone();
    let second = plan.clone();
    let one = std::thread::spawn(move || append_record(&first, &record_line(&["one"], 1, &[])));
    let two = std::thread::spawn(move || append_record(&second, &record_line(&["two"], 1, &[])));
    assert_eq!(one.join().unwrap()?, RecordWrite::Written);
    assert_eq!(two.join().unwrap()?, RecordWrite::Written);
    let content = fs::read_to_string(&plan)?;
    assert!(content.contains("Codex one"));
    assert!(content.contains("Codex two"));
    Ok(())
}

#[cfg(unix)]
#[test]
fn append_record_symlink_returns_plan_moved_without_changing_target() -> anyhow::Result<()> {
    let (dir, target) = plan_file("# Target\nKeep these bytes.\n");
    let link = dir.path().join("PLAN-link.md");
    symlink(&target, &link)?;
    let line = record_line(&["gpt-6.1-sol"], 1, &[]);

    let result = append_record(&link, &line)?;

    assert!(matches!(result, RecordWrite::PlanMoved));
    assert_eq!(fs::read(&target)?, b"# Target\nKeep these bytes.\n");
    assert_eq!(fs::read_link(&link)?, target);
    assert_eq!(
        fs::read_dir(dir.path())?.count(),
        2,
        "no temporary file created"
    );
    Ok(())
}

#[test]
fn append_record_missing_plan_returns_plan_moved_without_creating_files() -> anyhow::Result<()> {
    let dir = TempDir::new()?;
    let plan = dir.path().join("PLAN-missing.md");
    let line = record_line(&["gpt-6.1-sol"], 1, &[]);

    let result = append_record(&plan, &line)?;

    assert!(matches!(result, RecordWrite::PlanMoved));
    assert!(!plan.try_exists()?);
    assert_eq!(
        fs::read_dir(dir.path())?.count(),
        0,
        "directory stays empty"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn append_record_succeeds_when_plan_directory_cannot_be_opened() -> anyhow::Result<()> {
    let (dir, plan) = plan_file(END);
    // Write+search without read: the rename works, File::open(dir) fails.
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o311))?;
    let line = record_line(&["gpt-6.1-sol"], 1, &[]);

    let result = append_record(&plan, &line);

    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755))?;
    assert!(matches!(result?, RecordWrite::Written));
    assert_eq!(
        fs::read_to_string(&plan)?,
        format!("{END}\n## Pressure tests\n\n{RENDERED}\n")
    );
    Ok(())
}
