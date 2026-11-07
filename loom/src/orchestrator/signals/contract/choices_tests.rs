//! What the writer signal shows of the choices the writer makes at the freeze:
//! the plan's hint, `(you choose)`, a test an earlier freeze saved, the flags
//! the plan leaves open and rule 7.

use super::*;
use crate::models::stage::StageType;
use crate::plan::schema::{ContractChoice, ContractChoices};
use crate::verify::contracts::choices::save_choices;

fn spec(id: &str, test: Option<&str>) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: format!("src/{id}_contract_tests.rs"),
        test: test.map(str::to_string),
        runner: None,
        cwd: None,
        scenario: "s".to_string(),
        rejects: "r".to_string(),
    }
}

fn stage_of(contracts: Vec<ContractSpec>) -> Stage {
    Stage {
        id: "s1".to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        contracts,
        ..Stage::default()
    }
}

/// The signal of `stage` in a fresh work directory, after `before` ran on it.
fn signal(stage: &Stage, before: impl FnOnce(&Path)) -> String {
    let temp = tempfile::tempdir().unwrap();
    let work = temp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    before(&work);
    let worktree = Worktree::new("s1".to_string(), temp.path().to_path_buf(), "b".to_string());
    let session = Session::new_contract("s1");
    let path = generate_contract_signal(&session, stage, &worktree, &work, None, &[]).unwrap();
    std::fs::read_to_string(path).unwrap()
}

fn row<'a>(content: &'a str, id: &str) -> &'a str {
    let prefix = format!("| `{id}` |");
    content
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no row for {id}: {content}"))
}

#[test]
fn a_hint_is_shown_and_a_missing_test_reads_you_choose() {
    let stage = stage_of(vec![
        spec("hinted", Some("mod::hinted")),
        spec("open", None),
    ]);

    let content = signal(&stage, |_| {});

    assert!(
        row(&content, "hinted").contains("| `mod::hinted` |"),
        "{content}"
    );
    assert!(
        row(&content, "open").contains("| (you choose) |"),
        "{content}"
    );
    assert!(
        content.contains("- `open`: name its test with `--test open=<identifier>`; loom builds"),
        "{content}"
    );
    assert!(!content.contains("`open`: cannot be run"), "{content}");
}

#[test]
fn a_saved_choice_shows_as_the_test_chosen_by_an_earlier_freeze() {
    let stage = stage_of(vec![
        spec("hinted", Some("mod::hinted")),
        spec("open", None),
    ]);
    let saved: ContractChoices = [(
        "open".to_string(),
        ContractChoice {
            test: Some("mod::saved".to_string()),
            ..ContractChoice::default()
        },
    )]
    .into();

    let content = signal(&stage, |work| save_choices(work, "s1", &saved).unwrap());

    let open = row(&content, "open");
    assert!(
        open.contains("`mod::saved` (chosen by an earlier freeze)"),
        "{content}"
    );
    assert!(
        !row(&content, "hinted").contains("earlier freeze"),
        "{content}"
    );
    assert!(
        !content.contains("name its test with `--test open="),
        "{content}"
    );
}

#[test]
fn the_choices_paragraph_lists_what_the_plan_fixes() {
    let mut fixed = spec("fixed", Some("t"));
    fixed.runner = Some("cargo-test".to_string());
    let mut placed = spec("placed", Some("t"));
    placed.cwd = Some("pkg".to_string());
    let stage = stage_of(vec![fixed, placed, spec("free", None)]);

    let content = signal(&stage, |_| {});

    assert!(content.contains("### What you choose"), "{content}");
    assert!(
        content.contains("The plan fixes `runner` for `fixed` and `cwd` for `placed`."),
        "{content}"
    );
    assert!(content.contains("`--runner <id>=<adapter>`"), "{content}");
    assert!(content.contains("`--cwd <id>=<dir>`"), "{content}");
    assert!(
        content.contains("the daemon refuses a directory whose marker you added"),
        "{content}"
    );
}

#[test]
fn the_choices_paragraph_says_none_when_the_plan_fixes_nothing() {
    let content = signal(&stage_of(vec![spec("free", None)]), |_| {});

    assert!(
        content.contains("The plan fixes `runner` for none and `cwd` for none."),
        "{content}"
    );
}

#[test]
fn rule_seven_names_the_flags_and_the_wait() {
    let mut content = String::new();
    append_rules(&mut content, "s1");

    assert!(
        content.contains(
            "7. Finish with `loom stage contracts freeze s1 --test <id>=<identifier> ...`"
        ),
        "{content}"
    );
    assert!(content.contains("a later freeze keeps the flags it does not repeat"));
    assert!(content.contains("While `Host job:` reads `requested` or `pending`, change no file"));
    assert!(content.contains("the freeze runs the stage's formatter, lint and search checks"));
    assert!(content.contains(
        "A static refusal is returned to you; it is filed as a `freeze-refusal` dispute on the \
         third static refusal in the session, or when you stay quiet 5 minutes after a refusal"
    ));
}
