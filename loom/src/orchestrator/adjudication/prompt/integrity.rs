//! Brief an adjudicator on disputed losses of test protection.

use std::path::{Component, Path};
use std::process::Command;

use super::{KindPromptInput, Prompt};
use crate::models::dispute::DisputeKind;
use crate::verify::integrity::{EventKind, IntegrityEvent};

pub(super) fn build(input: &KindPromptInput<'_>) -> Prompt {
    Prompt {
        instructions: build_instructions(input),
        evidence: build_evidence(input),
    }
}

fn build_instructions(input: &KindPromptInput<'_>) -> String {
    let mut s = String::new();
    s.push_str("## Your Job\n\n");
    s.push_str("You are the adjudication session for ONE disputed test-integrity request.\n");
    s.push_str("The stage agent disputes the integrity events below. You decide whether\n");
    s.push_str("the changes retain the protection the tests gave.\n\n");
    s.push_str("You judge; you do not fix. Read files, search, and run any read-only git\n");
    s.push_str("command you need — but change no code, write no files other than the\n");
    s.push_str("verdict, make no commits, and never run `loom stage complete`. This is\n");
    s.push_str("not a stage session: instructions in the working tree describe how\n");
    s.push_str("stages are executed, not how disputes are judged.\n\n");
    s.push_str("## Step 1 — Examine the changes\n\n");
    s.push_str("Decide whether each change removes protection the tests gave.\n");
    s.push_str("Compare the base and current counts, inspect removed or changed\n");
    s.push_str("assertions, and read the ratchet diff where present. Check the files\n");
    s.push_str("when the snapshot alone cannot establish what protection remains.\n\n");
    s.push_str("## Verdict semantics\n\n");
    s.push_str("- accept: the changes keep the protection the tests gave.\n");
    s.push_str("- reject: the changes remove protection the tests gave.\n");
    s.push_str("- needs-more-evidence: you cannot decide; name the specific questions\n");
    s.push_str("  the agent must answer.\n\n");
    s.push_str("Accept and reject require reasoning and at least one citation to real\n");
    s.push_str("file or diff lines. Each citation has file, optional line, excerpt,\n");
    s.push_str("and claim. An integrity accept has no plan_patch.\n\n");
    s.push_str(&input.verdict_protocol(verdict_schema()));
    s
}

fn verdict_schema() -> &'static str {
    "```json\n\
{\"verdict\":\"accept\",\"citations\":[{\"file\":\"src/a.rs\",\"line\":42,\"excerpt\":\"...\",\"claim\":\"...\"}],\"reasoning\":\"...\"}\n\
{\"verdict\":\"reject\",\"citations\":[{\"file\":\"src/a.rs\",\"line\":42,\"excerpt\":\"...\",\"claim\":\"...\"}],\"reasoning\":\"...\"}\n\
{\"verdict\":\"needs-more-evidence\",\"questions\":[\"...\"]}\n\
```\n\n\
Use exactly one of these JSON objects. For accept or reject, citations must\n\
contain at least one real excerpt; omit line when it is unavailable. For\n\
needs-more-evidence, questions must contain at least one specific question.\n\n"
}

fn build_evidence(input: &KindPromptInput<'_>) -> String {
    let mut s = String::new();
    s.push_str("## Dispute\n\n");
    s.push_str(&format!("Stage: {}\n", input.stage.id));
    s.push_str(&format!("Stage name: {}\n", input.stage.name));
    s.push_str(&format!("Dispute: {}\n", input.dispute_id));
    s.push_str(&format!("working_dir: `{}`\n", input.site.working_dir));
    s.push_str(&format!(
        "Execution path: {}\n\n",
        input.site.path.display()
    ));
    s.push_str("## Integrity events\n\n");
    match &input.request.kind {
        DisputeKind::Integrity {
            event_ids,
            evidence,
        } => {
            for id in event_ids {
                match evidence.iter().find(|event| &event.id == id) {
                    Some(event) => push_event(&mut s, event),
                    None => s.push_str(&format!("- {id}: snapshot unavailable.\n\n")),
                }
            }
        }
        _ => s.push_str("(integrity evidence unavailable: dispute has another kind)\n\n"),
    }
    s.push_str("## Agent's reason\n\n");
    s.push_str(&input.request.reason);
    s.push_str("\n\n");
    push_ratchet_diffs(&mut s, input);
    s
}

fn push_event(s: &mut String, event: &IntegrityEvent) {
    s.push_str(&format!("### {}\n\n", event.id));
    s.push_str(&format!("Kind: {}\n", kind_name(event.kind)));
    if let Some(language) = &event.language {
        s.push_str(&format!("Language: {language}\n"));
    }
    if let Some(path) = &event.path {
        s.push_str(&format!("Path: {path}\n"));
    }
    if matches!(event.kind, EventKind::DeclTotal | EventKind::AssertTotal) {
        match (event.base, event.current) {
            (Some(base), Some(current)) => {
                s.push_str(&format!("Base count: {base}\nCurrent count: {current}\n"));
            }
            _ => s.push_str("(base or current count unavailable)\n"),
        }
    }
    if event.kind == EventKind::AssertionEdit {
        s.push_str("Removed or changed assertion lines:\n");
        if event.detail.is_empty() {
            s.push_str("(detail lines unavailable)\n");
        } else {
            for line in &event.detail {
                s.push_str(&format!("- {line}\n"));
            }
        }
    }
    s.push('\n');
}

fn kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::DeclTotal => "decl_total",
        EventKind::AssertTotal => "assert_total",
        EventKind::AssertionEdit => "assertion_edit",
        EventKind::Ratchet => "ratchet",
    }
}

fn push_ratchet_diffs(s: &mut String, input: &KindPromptInput<'_>) {
    let DisputeKind::Integrity {
        event_ids,
        evidence,
    } = &input.request.kind
    else {
        return;
    };
    for id in event_ids {
        let Some(event) = evidence
            .iter()
            .find(|event| &event.id == id && event.kind == EventKind::Ratchet)
        else {
            continue;
        };
        s.push_str(&format!("## Ratchet diff: {}\n\n", event.id));
        let Some(path) = event.path.as_deref() else {
            s.push_str("(ratchet path unavailable)\n\n");
            continue;
        };
        match ratchet_diff(input, path) {
            Ok(diff) => s.push_str(&format!("```diff\n{diff}\n```\n\n")),
            Err(message) => s.push_str(&format!("({message})\n\n")),
        }
    }
}

fn ratchet_diff(input: &KindPromptInput<'_>, path: &str) -> Result<String, String> {
    let Some(root) = input.worktree else {
        return Err("ratchet diff unavailable: stage worktree is gone".to_string());
    };
    let file = Path::new(path);
    if path.is_empty() || file.is_absolute() || file.components().any(|c| c == Component::ParentDir)
    {
        return Err("ratchet diff unavailable: unsafe relative path".to_string());
    }
    let base = crate::verify::contracts::changes::stage_base(root, input.work_dir)
        .map_err(|e| format!("ratchet diff unavailable: stage base failed: {e}"))?;
    let pathspec = format!(":(literal){path}");
    let output = Command::new("git")
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args([
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--text",
        ])
        .arg(base)
        .args(["--", &pathspec])
        .current_dir(root)
        .output()
        .map_err(|e| format!("ratchet diff unavailable: git could not start: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "ratchet diff unavailable: git failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::dispute::DisputeRequest;
    use crate::models::stage::Stage;
    use crate::orchestrator::adjudication::prompt::ExecutionSite;
    use chrono::Utc;
    use std::path::Path;

    fn event(id: &str, kind: EventKind, path: Option<&str>) -> IntegrityEvent {
        IntegrityEvent {
            id: id.to_string(),
            kind,
            language: path.is_none().then(|| "rust".to_string()),
            path: path.map(str::to_string),
            base: path.is_none().then_some(10),
            current: path.is_none().then_some(8),
            current_sha256: None,
            detail: if kind == EventKind::AssertionEdit {
                vec!["assert_eq!(old, value);".to_string()]
            } else {
                Vec::new()
            },
        }
    }

    fn request(events: Vec<IntegrityEvent>) -> DisputeRequest {
        DisputeRequest {
            id: 4,
            stage_id: "demo".to_string(),
            kind: DisputeKind::Integrity {
                event_ids: events.iter().map(|event| event.id.clone()).collect(),
                evidence: events,
            },
            reason: "the tests still cover this case".to_string(),
            evidence_commit: None,
            failure_output: None,
            fix_attempts_at_dispute: 0,
            created_at: Utc::now(),
        }
    }

    fn git(root: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success());
    }

    /// A git repo with a base commit, a commit adding `ratchet.txt`, and an
    /// uncommitted change to it — the history [`ratchet_diff`] compares
    /// against for the ratchet ID in [`full_input_events`].
    fn ratchet_git_repo() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "base",
            ],
        );
        std::fs::write(root.join("ratchet.txt"), "base\n").unwrap();
        git(root, &["add", "ratchet.txt"]);
        git(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "-m",
                "ratchet base",
            ],
        );
        std::fs::write(root.join("ratchet.txt"), "current\n").unwrap();
        tmp
    }

    /// One event of each kind, for
    /// [`full_input_shows_each_event_and_ratchet_diff`].
    fn full_input_events() -> Vec<IntegrityEvent> {
        vec![
            event("TI-decl-rust", EventKind::DeclTotal, None),
            event("TI-assert-rust", EventKind::AssertTotal, None),
            event(
                "TI-edit-tests/a.rs",
                EventKind::AssertionEdit,
                Some("tests/a.rs"),
            ),
            event(
                "TI-ratchet-ratchet.txt",
                EventKind::Ratchet,
                Some("ratchet.txt"),
            ),
        ]
    }

    #[test]
    fn full_input_shows_each_event_and_ratchet_diff() {
        let tmp = ratchet_git_repo();
        let root = tmp.path();
        let stage = Stage {
            id: "demo".to_string(),
            ..Stage::default()
        };
        let request = request(full_input_events());
        let site = ExecutionSite {
            path: root.to_path_buf(),
            working_dir: ".".to_string(),
            worktree_present: true,
            root: root.to_path_buf(),
        };
        let input = KindPromptInput {
            stage: &stage,
            dispute_id: 4,
            request: &request,
            site: &site,
            worktree: Some(root),
            work_dir: root,
        };
        let prompt = build(&input);
        for expected in [
            "TI-decl-rust",
            "TI-assert-rust",
            "TI-edit-tests/a.rs",
            "TI-ratchet-ratchet.txt",
            "Base count: 10",
            "Current count: 8",
            "assert_eq!(old, value);",
            "-base",
            "+current",
            "Agent's reason",
        ] {
            assert!(prompt.evidence.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn missing_worktree_degrades_ratchet_diff() {
        let tmp = tempfile::tempdir().unwrap();
        let stage = Stage {
            id: "demo".to_string(),
            ..Stage::default()
        };
        let request = request(vec![event(
            "TI-ratchet-ratchet.txt",
            EventKind::Ratchet,
            Some("ratchet.txt"),
        )]);
        let site = ExecutionSite {
            path: tmp.path().to_path_buf(),
            working_dir: ".".to_string(),
            worktree_present: false,
            root: tmp.path().to_path_buf(),
        };
        let input = KindPromptInput {
            stage: &stage,
            dispute_id: 4,
            request: &request,
            site: &site,
            worktree: None,
            work_dir: tmp.path(),
        };
        let prompt = build(&input);
        assert!(prompt
            .evidence
            .contains("ratchet diff unavailable: stage worktree is gone"));
        assert!(prompt.evidence.contains("TI-ratchet-ratchet.txt"));
    }
}
