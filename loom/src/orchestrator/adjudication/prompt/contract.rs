//! Briefing for a dispute over one frozen behavioural contract.

use std::fs;
use std::path::Path;

use super::{KindPromptInput, Prompt};
use crate::fs::safe_read::{is_not_found, read_bounded};
use crate::models::dispute::DisputeKind;
use crate::plan::schema::ContractSpec;
use crate::relay::sha256_hex;
use crate::verify::contracts::run_dir::{resolve_run, ContractRun};
use crate::verify::contracts::store::FreezeRecord;
use crate::verify::contracts::{normalize, resolve_adapter, store};

const FROZEN_SET_HEADING: &str = "## Other frozen files changed since the freeze";
/// Bounds on the frozen-set evidence: one file's diff, and the whole section.
const MAX_FROZEN_SET_DIFF_BYTES: usize = 8 * 1024;
const MAX_FROZEN_SET_SECTION_BYTES: usize = 32 * 1024;

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
    s.push_str("  spec itself needs a replacement in the stage's contracts array. An accept\n");
    s.push_str("  re-freezes every contract and harness file at its current content and\n");
    s.push_str("  drops deleted harness files, so judge the changes listed under\n");
    s.push_str(&format!("  `{FROZEN_SET_HEADING}` too.\n"));
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
    match (spec(input), input.worktree) {
        (Some(contract), _) if !valid_file(&contract.file) => {
            s.push_str("(contract file path is unsafe; do not run a command against it)\n\n");
        }
        (Some(contract), Some(worktree)) => match contract_run(input, contract, worktree) {
            Ok(run) => {
                let quoted_dir = run.dir.display().to_string().replace('\'', "'\\''");
                s.push_str(&format!(
                    "Run from the contract's run directory (`{}` in the stage's worktree), \
                     where the completion check runs it:\n\n",
                    run.cwd
                ));
                s.push_str(&format!(
                    "```bash\ncd -- '{quoted_dir}'\n{}\necho \"exit: $?\"\n```\n\n",
                    run.command
                ));
            }
            Err(error) => s.push_str(&format!(
                "(the contract's run directory cannot be resolved: {error:#}; report what \
                 evidence is missing)\n\n"
            )),
        },
        (Some(_), None) => {
            s.push_str("(stage worktree unavailable; report what evidence is missing)\n\n")
        }
        (None, _) => {
            s.push_str("(disputed contract spec unavailable; report what evidence is missing)\n\n")
        }
    }
    s.push_str("A failed run alone does not decide whether the change weakens the contract.\n");
    s.push_str("Compare the scenario and rejects clause with both versions of the file.\n\n");
    s
}

/// Where and how the completion check runs `contract`, from the run
/// directory the plan gives or the freeze recorded for its file.
fn contract_run(
    input: &KindPromptInput<'_>,
    contract: &ContractSpec,
    worktree: &Path,
) -> anyhow::Result<ContractRun> {
    let recorded = recorded_cwd(input, contract)?;
    let adapter = resolve_adapter(contract, &input.site.path);
    resolve_run(
        contract,
        adapter,
        worktree,
        &input.site.path,
        recorded.as_deref(),
    )
}

/// The run directory the freeze recorded for `contract`'s file, if it
/// recorded one.
fn recorded_cwd(
    input: &KindPromptInput<'_>,
    contract: &ContractSpec,
) -> anyhow::Result<Option<String>> {
    let record = store::load_freeze(input.work_dir, &input.stage.id)?;
    Ok(record.and_then(|record| store::recorded_cwd(&record, contract).map(str::to_string)))
}

/// The disputed contract's spec as the plan holds it, with the run directory
/// the plan gives and the one the freeze recorded.
fn spec_block(input: &KindPromptInput<'_>, contract: &ContractSpec) -> String {
    let recorded = match recorded_cwd(input, contract) {
        Ok(Some(cwd)) => cwd,
        Ok(None) => "(none recorded)".to_string(),
        Err(error) => format!("(freeze record unreadable: {error:#})"),
    };
    format!(
        "id: {}\nfile: {}\ntest: {}\ncwd: {}\nrecorded cwd: {recorded}\nscenario: {}\n\
         rejects: {}\n\n",
        contract.id,
        contract.file,
        contract.test,
        contract.cwd.as_deref().unwrap_or("(not set; detected)"),
        contract.scenario,
        contract.rejects
    )
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
YAML (including id, file, test, scenario, and rejects, plus `runner` and `cwd`
whenever the current spec sets them). `cwd` is the optional directory the
contract runs from, relative to the worktree root, and always decides where it
runs. Without one, a replacement that keeps `file` keeps the recorded cwd, and
one that changes `file` lets loom detect the directory again.
Accept and reject each need at least one citation; needs-more-evidence needs
at least one question.

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
        s.push_str(&spec_block(input, contract));
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
    s.push_str(&frozen_set_section(input, contract));
    s
}

/// Every frozen file other than the disputed contract's whose worktree bytes
/// differ from the freeze: a bounded diff of each, and a line naming each
/// deleted one. An accept re-freezes the changed files and drops the deleted.
fn frozen_set_section(input: &KindPromptInput<'_>, contract: Option<&ContractSpec>) -> String {
    let record = store::load_freeze(input.work_dir, &input.stage.id);
    let body = match (record, input.worktree) {
        (_, None) => "(stage worktree unavailable)\n\n".to_string(),
        (Ok(None), _) => "(the contracts are not frozen)\n\n".to_string(),
        (Err(error), _) => format!("(freeze record unreadable: {error:#})\n\n"),
        (Ok(Some(record)), Some(_)) => {
            let disputed = contract.map(|contract| normalize(&contract.file));
            changed_frozen_files(input, &record, disputed.as_deref())
        }
    };
    format!("{FROZEN_SET_HEADING}\n\n{body}")
}

fn changed_frozen_files(
    input: &KindPromptInput<'_>,
    record: &FreezeRecord,
    disputed: Option<&str>,
) -> String {
    let mut out = String::new();
    for file in record
        .files
        .iter()
        .filter(|file| Some(file.path.as_str()) != disputed)
    {
        let path = &file.path;
        let current = read_bounded(
            &input.site.path,
            Path::new(path),
            store::MAX_FROZEN_FILE_BYTES,
        );
        let entry = match current {
            Ok(bytes) if sha256_hex(&bytes) == file.sha256 => continue,
            Ok(bytes) => changed_entry(input, path, &bytes),
            Err(error) if is_not_found(&error) => {
                format!("- `{path}` is deleted from the worktree\n\n")
            }
            Err(error) => format!("- `{path}` is unreadable in the worktree: {error:#}\n\n"),
        };
        if out.len() + entry.len() > MAX_FROZEN_SET_SECTION_BYTES {
            out.push_str("(more changed frozen files omitted; compare them in the worktree)\n\n");
            break;
        }
        out.push_str(&entry);
    }
    if out.is_empty() {
        out.push_str("(none)\n\n");
    }
    out
}

/// The diff of the frozen copy of `path` against its `current` bytes, cut to
/// [`MAX_FROZEN_SET_DIFF_BYTES`].
fn changed_entry(input: &KindPromptInput<'_>, path: &str, current: &[u8]) -> String {
    let frozen = frozen_copy(input, path).ok();
    let before = frozen
        .as_deref()
        .and_then(|bytes| std::str::from_utf8(bytes).ok());
    let (Some(before), Ok(after)) = (before, std::str::from_utf8(current)) else {
        return format!("- `{path}` changed (no text diff: a side is unreadable or not UTF-8)\n\n");
    };
    let mut diff = super::diff::unified_diff(before, after, path);
    if diff.len() > MAX_FROZEN_SET_DIFF_BYTES {
        let mut end = MAX_FROZEN_SET_DIFF_BYTES;
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        diff.truncate(end);
        diff.push_str("\n... [diff truncated] ...\n");
    }
    format!("- `{path}` changed:\n\n```diff\n{diff}```\n\n")
}

/// The frozen copy of `path`, read without following a symlink.
fn frozen_copy(input: &KindPromptInput<'_>, path: &str) -> anyhow::Result<Vec<u8>> {
    let root = input.work_dir.canonicalize()?;
    let copy = store::frozen_file_path(&root, &input.stage.id, path);
    let relative = copy.strip_prefix(&root)?;
    read_bounded(&root, relative, store::MAX_FROZEN_FILE_BYTES)
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
#[path = "contract_tests.rs"]
mod tests;
