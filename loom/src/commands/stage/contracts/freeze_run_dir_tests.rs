//! A contract whose file sits in a sub-workspace with its own runner config:
//! the root `bunfig.toml` ignores `apps/web`, and `apps/web/bunfig.toml`
//! preloads the setup the test needs, so only a run from `apps/web` selects
//! and sets up the test. The freeze runs it red from there, and the
//! completion check runs it green from there once implemented, whether the
//! freeze record names the run directory or leaves it to detection.

use super::*;
use crate::models::stage::Stage;
use crate::relay::sha256_hex;
use crate::verify::contracts::completion;
use crate::verify::contracts::store::{
    write_freeze, FreezeRecord, FrozenContract, FrozenFile, FREEZE_RECORD_VERSION,
};
use chrono::Utc;
use std::path::PathBuf;
use tempfile::TempDir;

const FILE: &str = "apps/web/src/a.contract.test.ts";
const CONTRACT_TEST: &str = r#"import { expect, test } from "bun:test";
import { widget } from "./widget";

test("widget renders", () => {
  expect((globalThis as { __dom?: { ready: boolean } }).__dom?.ready).toBe(true);
  expect(widget()).toBe("rendered");
});
"#;

fn bun_missing() -> bool {
    let missing = which::which("bun").is_err();
    if missing {
        eprintln!("skipping: `bun` is not on PATH");
    }
    missing
}

fn spec() -> ContractSpec {
    ContractSpec {
        id: "a".to_string(),
        file: FILE.to_string(),
        test: "widget renders".to_string(),
        runner: Some("bun-test".to_string()),
        cwd: None,
        scenario: "the widget renders in a DOM set up by the web preload".to_string(),
        rejects: "a widget that still returns its placeholder".to_string(),
    }
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn widget(root: &Path, returns: &str) {
    let source = format!("export function widget(): string {{\n  return \"{returns}\";\n}}\n");
    write(root, "apps/web/src/widget.ts", &source);
}

/// The repro layout, with the widget not yet implemented.
fn layout(root: &Path) {
    write(
        root,
        "bunfig.toml",
        "[test]\npathIgnorePatterns = [\"apps/web/**\"]\n",
    );
    write(
        root,
        "apps/web/bunfig.toml",
        "[test]\npreload = [\"./src/test/setup.ts\"]\n",
    );
    write(
        root,
        "apps/web/src/test/setup.ts",
        "(globalThis as { __dom?: { ready: boolean } }).__dom = { ready: true };\n",
    );
    widget(root, "todo");
    write(root, FILE, CONTRACT_TEST);
}

/// Freeze the contract file with `contract` as its run's record, as the
/// freeze handler writes it, into a fresh state directory `tmp/<name>`.
fn freeze_into(tmp: &Path, name: &str, contract: FrozenContract) -> PathBuf {
    let work_dir = tmp.join(name);
    std::fs::create_dir_all(&work_dir).unwrap();
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: "web".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "a".repeat(40),
        files: vec![FrozenFile {
            path: FILE.to_string(),
            sha256: sha256_hex(CONTRACT_TEST.as_bytes()),
        }],
        contracts: vec![contract],
    };
    let copies = [(FILE.to_string(), CONTRACT_TEST.as_bytes().to_vec())];
    write_freeze(&work_dir, &record, &copies).unwrap();
    work_dir
}

/// The freeze CLI's red run of the contract, recorded with the run directory
/// the freeze handler resolves.
fn freeze_red(root: &Path) -> FrozenContract {
    let contract = spec();
    let judged = run_contract(root, root, &contract, CommandConfinement::Confined).unwrap();
    let report = judged.unwrap_or_else(|problem| panic!("freeze refused: {problem}"));
    assert_eq!(report.outcome, "failed", "{report:?}");
    let adapter = resolve_adapter(&contract, root);
    let run = resolve_run(&contract, adapter, root, root, None).unwrap();
    assert_eq!(run.cwd, "apps/web");
    FrozenContract {
        file: Some(FILE.to_string()),
        cwd: Some(run.cwd),
        ..FrozenContract::from(&report)
    }
}

#[test]
fn a_sub_workspace_contract_freezes_red_and_completes_green_from_its_config_dir() {
    if bun_missing() {
        return;
    }
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("repo");
    layout(&root);
    let frozen = freeze_red(&root);
    let recorded = freeze_into(tmp.path(), "work-recorded", frozen.clone());
    let detected = freeze_into(
        tmp.path(),
        "work-detected",
        FrozenContract {
            file: None,
            cwd: None,
            ..frozen
        },
    );

    widget(&root, "rendered");

    let current = std::fs::read(root.join(FILE)).unwrap();
    assert_eq!(sha256_hex(&current), sha256_hex(CONTRACT_TEST.as_bytes()));
    let stage = Stage {
        id: "web".to_string(),
        plan_version: 2,
        contracts: vec![spec()],
        ..Stage::default()
    };
    for work_dir in [&recorded, &detected] {
        completion::check(&stage, work_dir, &root, &root)
            .unwrap_or_else(|error| panic!("{}: {error:#}", work_dir.display()));
    }
}
