//! Briefing for a dispute over one frozen behavioural contract.

use std::fs;
use std::path::Path;

use super::{KindPromptInput, Prompt};
use crate::models::dispute::DisputeKind;
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::{contract_command, resolve_adapter, store};

pub(super) fn build(input: &KindPromptInput<'_>) -> Prompt {
    Prompt {
        instructions: build_instructions(input),
        evidence: build_evidence(input),
    }
}

fn spec<'a>(input: &'a KindPromptInput<'_>) -> Option<&'a ContractSpec> {
    let DisputeKind::Contract { contract_id } = &input.request.kind else {
        return None;
    };
    input
        .stage
        .contracts
        .iter()
        .find(|spec| spec.id == contract_id.as_str())
}

fn build_instructions(input: &KindPromptInput<'_>) -> String {
    let mut s = String::new();
    s.push_str("## Your Job\n\n");
    s.push_str("You are the adjudication session for ONE disputed frozen contract.\n");
    s.push_str("The stage agent disputes a change to a contract test. Decide whether\n");
    s.push_str("the current test still enforces its scenario and rejects clause.\n\n");
    s.push_str("You judge; you do not fix. Read files and run the contract test, but change\n");
    s.push_str("no code, write no files other than the verdict, make no commits, and never\n");
    s.push_str("run `loom stage complete`. Working-tree stage instructions do not govern\n");
    s.push_str("this adjudication session.\n\n");
    s.push_str(&first_step(input));
    s.push_str("## Verdict semantics\n\n");
    s.push_str("- accept: the changed test still covers the scenario and rejects the stated\n");
    s.push_str("  plausible wrong implementation. Omit plan_patch unless the contract\n");
    s.push_str("  spec itself needs a replacement in the stage's contracts array.\n");
    s.push_str("- reject: the change weakens the contract's scenario or rejects clause.\n");
    s.push_str("- needs-more-evidence: you cannot decide; list specific questions.\n\n");
    s.push_str("Accept and reject require reasoning and citations to real lines in the\n");
    s.push_str("contract, frozen or current file, or diff. A citation has file, optional\n");
    s.push_str("line, excerpt, and claim. Include the command, exit code, and decisive\n");
    s.push_str("output from your test run in a citation when you can run it.\n\n");
    s.push_str(&input.verdict_protocol(verdict_schema()));
    s
}

fn first_step(input: &KindPromptInput<'_>) -> String {
    let mut s = String::from("## Step 1 — Run and compare the contract\n\n");
    s.push_str("Run the contract test as the adapter runs it, then compare the frozen content with the current content.\n\n");
    match spec(input) {
        Some(contract) if input.worktree.is_some() && valid_file(&contract.file) => {
            let dir = &input.site.path;
            let command = contract_command(contract, resolve_adapter(contract, dir), dir);
            let quoted_dir = dir.display().to_string().replace('\'', "'\\''");
            s.push_str(&format!(
                "Run from the stage's worktree plus `working_dir` (`{}`):\n\n",
                input.site.working_dir
            ));
            s.push_str(&format!(
                "```bash\ncd -- '{quoted_dir}'\n{command}\necho \"exit: $?\"\n```\n\n"
            ));
        }
        Some(contract) if !valid_file(&contract.file) => {
            s.push_str("(contract file path is unsafe; do not run a command against it)\n\n");
        }
        Some(_) => s.push_str("(stage worktree unavailable; report what evidence is missing)\n\n"),
        None => {
            s.push_str("(disputed contract spec unavailable; report what evidence is missing)\n\n")
        }
    }
    s.push_str("A failed run alone does not decide whether the change weakens the contract.\n");
    s.push_str("Compare the scenario and rejects clause with both versions of the file.\n\n");
    s
}

fn verdict_schema() -> &'static str {
    r#"```json
{"verdict":"accept","plan_patch":{"field":"contracts","patch":{"op":"replace","index":0,"value":"id: rejects-x\nfile: tests/x_contract.rs\ntest: rejects_x\nscenario: x arrives\nrejects: accepting x"},"reason":"why the spec changes"},"citations":[{"file":"tests/x_contract.rs","line":1,"excerpt":"...","claim":"..."}],"reasoning":"..."}
{"verdict":"reject","citations":[{"file":"tests/x_contract.rs","line":1,"excerpt":"...","claim":"..."}],"reasoning":"..."}
{"verdict":"needs-more-evidence","questions":["..."]}
```

Write one of these JSON objects. On accept, omit `plan_patch` unless the
contract spec itself needs replacement. If included, `field` must be
`contracts`, `index` is the zero-based index in the stage's contracts array,
and `value` is a JSON string containing the complete replacement contract in
YAML (including id, file, test, scenario, and rejects). Accept and reject each
need at least one citation; needs-more-evidence needs at least one question.

"#
}

fn build_evidence(input: &KindPromptInput<'_>) -> String {
    let id = match &input.request.kind {
        DisputeKind::Contract { contract_id } => contract_id.as_str(),
        _ => "(non-contract dispute)",
    };
    let mut s = format!(
        "## Dispute\n\nStage: {}\nStage name: {}\nContract ID: {}\nworking_dir: `{}`\nExecution path: {}\n\n",
        input.stage.id, input.stage.name, id, input.site.working_dir, input.site.path.display()
    );
    let contract = spec(input);
    s.push_str("## Contract spec\n\n");
    if let Some(contract) = contract {
        s.push_str(&format!(
            "id: {}\nfile: {}\ntest: {}\nscenario: {}\nrejects: {}\n\n",
            contract.id, contract.file, contract.test, contract.scenario, contract.rejects
        ));
    } else {
        s.push_str("(contract spec unavailable for this dispute)\n\n");
    }
    let frozen = frozen_content(input, contract);
    let current = current_content(input, contract);
    push_content(&mut s, "Frozen content", &frozen);
    push_content(&mut s, "Current content", &current);
    s.push_str("## Agent's reason\n\n");
    s.push_str(&input.request.reason);
    s.push_str("\n\n## Diff (frozen vs current)\n\n");
    match (frozen, current, contract) {
        (Ok(before), Ok(after), Some(contract)) => {
            s.push_str("```diff\n");
            s.push_str(&super::diff::unified_diff(&before, &after, &contract.file));
            s.push_str("```\n\n");
        }
        _ => s.push_str("(diff unavailable because frozen or current content is unavailable)\n\n"),
    }
    s
}

fn valid_file(path: &str) -> bool {
    store::validate_relative(path).is_ok()
}

fn frozen_content(
    input: &KindPromptInput<'_>,
    contract: Option<&ContractSpec>,
) -> Result<String, String> {
    let Some(contract) = contract else {
        return Err("(frozen file unavailable: contract spec missing)".to_string());
    };
    if !valid_file(&contract.file) {
        return Err("(frozen file unavailable: unsafe contract file path)".to_string());
    }
    let root = input
        .work_dir
        .join("contracts")
        .join(&input.stage.id)
        .join("files");
    let work_dir = fs::canonicalize(input.work_dir)
        .map_err(|_| "(frozen file work directory unavailable)".to_string())?;
    let frozen_root =
        fs::canonicalize(&root).map_err(|_| "(frozen file root unavailable)".to_string())?;
    if !frozen_root.starts_with(work_dir) {
        return Err("(frozen file root escapes the work directory)".to_string());
    }
    let path = store::frozen_file_path(input.work_dir, &input.stage.id, &contract.file);
    read_confined(&frozen_root, &path, "frozen file")
}

fn current_content(
    input: &KindPromptInput<'_>,
    contract: Option<&ContractSpec>,
) -> Result<String, String> {
    let Some(contract) = contract else {
        return Err("(current file unavailable: contract spec missing)".to_string());
    };
    if !valid_file(&contract.file) {
        return Err("(current file unavailable: unsafe contract file path)".to_string());
    }
    let Some(worktree) = input.worktree else {
        return Err("(current file unavailable: stage worktree missing)".to_string());
    };
    read_confined(
        worktree,
        &input.site.path.join(&contract.file),
        "current file",
    )
}

fn read_confined(root: &Path, path: &Path, label: &str) -> Result<String, String> {
    let root = fs::canonicalize(root).map_err(|_| format!("({label} root unavailable)"))?;
    let path = fs::canonicalize(path).map_err(|_| format!("({label} unavailable)"))?;
    if !path.starts_with(&root) {
        return Err(format!("({label} path escapes its root)"));
    }
    fs::read_to_string(path).map_err(|_| format!("({label} unreadable as UTF-8 text)"))
}

fn push_content(out: &mut String, heading: &str, content: &Result<String, String>) {
    out.push_str(&format!("## {heading}\n\n"));
    match content {
        Ok(text) => {
            out.push_str("```text\n");
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n\n");
        }
        Err(message) => out.push_str(&format!("{message}\n\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::dispute::DisputeRequest;
    use crate::models::stage::Stage;
    use crate::orchestrator::adjudication::prompt::ExecutionSite;
    use crate::verify::contracts::test_support::contract;
    use chrono::Utc;
    use std::path::PathBuf;

    fn request() -> DisputeRequest {
        DisputeRequest {
            id: 4,
            stage_id: "demo".to_string(),
            kind: DisputeKind::Contract {
                contract_id: "rejects-x".to_string(),
            },
            reason: "the new assertion still rejects x".to_string(),
            evidence_commit: None,
            failure_output: None,
            fix_attempts_at_dispute: 0,
            created_at: Utc::now(),
        }
    }

    /// Fixture for [`full_briefing_quotes_each_source_and_the_diff`]: a
    /// stage with one contract, its frozen file and current worktree file
    /// written to disk, ready for [`KindPromptInput`] to borrow from.
    struct ContractFixture {
        _tmp: tempfile::TempDir,
        work_dir: PathBuf,
        worktree: PathBuf,
        stage: Stage,
        request: DisputeRequest,
        site: ExecutionSite,
    }

    fn contract_fixture() -> ContractFixture {
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = tmp.path().join(".loom/work");
        let worktree = tmp.path().join(".worktrees/demo");
        let execution = worktree.join("loom");
        let stage = Stage {
            id: "demo".to_string(),
            working_dir: Some("loom".to_string()),
            contracts: vec![contract()],
            ..Stage::default()
        };
        let frozen = store::frozen_file_path(&work_dir, &stage.id, &stage.contracts[0].file);
        let current = execution.join(&stage.contracts[0].file);
        fs::create_dir_all(frozen.parent().unwrap()).unwrap();
        fs::create_dir_all(current.parent().unwrap()).unwrap();
        fs::write(&frozen, "assert!(false);\n").unwrap();
        fs::write(&current, "assert!(rejects_x());\n").unwrap();
        let site = ExecutionSite {
            path: execution,
            working_dir: "loom".to_string(),
            worktree_present: true,
            root: worktree.clone(),
        };
        ContractFixture {
            _tmp: tmp,
            work_dir,
            worktree,
            stage,
            request: request(),
            site,
        }
    }

    #[test]
    fn full_briefing_quotes_each_source_and_the_diff() {
        let fx = contract_fixture();
        let input = KindPromptInput {
            stage: &fx.stage,
            dispute_id: 4,
            request: &fx.request,
            site: &fx.site,
            worktree: Some(&fx.worktree),
            work_dir: &fx.work_dir,
        };

        let prompt = build(&input);

        for expected in [
            "## Contract spec",
            "id: rejects-x",
            "file: tests/x_contract.rs",
            "test: tests::rejects_x",
            "scenario: x arrives",
            "rejects: accepting x",
            "## Frozen content",
            "assert!(false);",
            "## Current content",
            "assert!(rejects_x());",
            "## Agent's reason",
            "the new assertion still rejects x",
            "## Diff (frozen vs current)",
            "-assert!(false);",
            "+assert!(rejects_x());",
        ] {
            assert!(prompt.evidence.contains(expected), "missing {expected}");
        }
        assert!(prompt
            .instructions
            .contains("loom stage adjudicate --stage demo"));
    }

    #[test]
    fn missing_worktree_leaves_a_readable_briefing() {
        let tmp = tempfile::tempdir().unwrap();
        let work_dir = tmp.path().join(".loom/work");
        let stage = Stage {
            id: "demo".to_string(),
            contracts: vec![contract()],
            ..Stage::default()
        };
        let site = ExecutionSite {
            path: tmp.path().to_path_buf(),
            working_dir: ".".to_string(),
            worktree_present: false,
            root: tmp.path().to_path_buf(),
        };
        let request = request();
        let input = KindPromptInput {
            stage: &stage,
            dispute_id: 4,
            request: &request,
            site: &site,
            worktree: None,
            work_dir: &work_dir,
        };

        let prompt = build(&input);

        assert!(prompt
            .evidence
            .contains("(current file unavailable: stage worktree missing)"));
        assert!(prompt.evidence.contains("(diff unavailable"));
        assert!(prompt.instructions.contains("stage worktree unavailable"));
    }
}
