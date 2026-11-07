use super::*;
use crate::plan::schema::StageDefinition;

fn spec(test: Option<&str>, runner: Option<&str>, cwd: Option<&str>) -> ContractSpec {
    ContractSpec {
        id: "c".to_string(),
        file: "tests/c.rs".to_string(),
        test: test.map(str::to_string),
        runner: runner.map(str::to_string),
        cwd: cwd.map(str::to_string),
        scenario: "s".to_string(),
        rejects: "r".to_string(),
    }
}

fn choice(test: Option<&str>, runner: Option<&str>, cwd: Option<&str>) -> ContractChoice {
    ContractChoice {
        test: test.map(str::to_string),
        runner: runner.map(str::to_string),
        cwd: cwd.map(str::to_string),
    }
}

#[test]
fn with_choice_covers_every_runner_and_cwd_combination() {
    for planner_runner in [None, Some("cargo")] {
        for planner_cwd in [None, Some("pkg")] {
            for choice_runner in [None, Some("vitest")] {
                for choice_cwd in [None, Some("other")] {
                    let base = spec(None, planner_runner, planner_cwd);
                    let applied = base.with_choice(&choice(None, choice_runner, choice_cwd));
                    assert_eq!(
                        applied.runner.as_deref(),
                        planner_runner.or(choice_runner),
                        "runner for planner {planner_runner:?} choice {choice_runner:?}"
                    );
                    assert_eq!(
                        applied.cwd.as_deref(),
                        planner_cwd.or(choice_cwd),
                        "cwd for planner {planner_cwd:?} choice {choice_cwd:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn with_choice_test_from_choice_beats_hint() {
    let hinted = spec(Some("hint"), None, None);
    let applied = hinted.with_choice(&choice(Some("chosen"), None, None));
    assert_eq!(applied.test.as_deref(), Some("chosen"));
    let kept = hinted.with_choice(&ContractChoice::default());
    assert_eq!(kept.test.as_deref(), Some("hint"));
    let none = spec(None, None, None).with_choice(&ContractChoice::default());
    assert_eq!(none.test, None);
}

#[test]
fn required_test_names_the_contract_when_missing() {
    assert_eq!(spec(Some("t"), None, None).required_test().unwrap(), "t");
    let error = spec(None, None, None)
        .required_test()
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("contract 'c' has no test identifier"),
        "{error}"
    );
    assert!(error.contains("--test c=<identifier>"), "{error}");
}

#[test]
fn spec_without_test_round_trips_through_yaml() {
    let yaml = "id: c\nfile: tests/c.rs\nscenario: s\nrejects: r\n";
    let parsed: ContractSpec = crate::yaml::from_str(yaml).unwrap();
    assert_eq!(parsed.test, None);
    let rendered = crate::yaml::to_string(&parsed).unwrap();
    assert!(
        !rendered.lines().any(|line| line.starts_with("test:")),
        "{rendered}"
    );
    assert_eq!(
        crate::yaml::from_str::<ContractSpec>(&rendered).unwrap(),
        parsed
    );
    let with_test: ContractSpec = crate::yaml::from_str(&format!("{yaml}test: t::x\n")).unwrap();
    assert_eq!(with_test.test.as_deref(), Some("t::x"));
}

fn parse_deferred(value: &str) -> Result<StageDefinition, serde_json::Error> {
    serde_json::from_value(serde_json::json!({
        "id": "s",
        "name": "S",
        "working_dir": ".",
        "consumer_deferred": value,
    }))
}

#[test]
fn consumer_deferred_refuses_empty_long_and_control_characters() {
    for bad in [
        "".to_string(),
        "   ".to_string(),
        "a".repeat(301),
        "one\ntwo".to_string(),
        "one\ttwo".to_string(),
    ] {
        let error = parse_deferred(&bad).unwrap_err().to_string();
        assert!(
            error.contains("consumer_deferred must be one line of at most 300 bytes"),
            "{bad:?}: {error}"
        );
    }
}

#[test]
fn consumer_deferred_accepts_300_bytes_and_stores_it_trimmed() {
    let exact = "a".repeat(300);
    let parsed = parse_deferred(&format!("  {exact}  ")).unwrap();
    assert_eq!(parsed.consumer_deferred.as_deref(), Some(exact.as_str()));
    let omitted: StageDefinition =
        crate::yaml::from_str("id: s\nname: S\nworking_dir: .\n").unwrap();
    assert_eq!(omitted.consumer_deferred, None);
}
