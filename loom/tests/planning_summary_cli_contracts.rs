//! Contracts for stage summary-store: store and draft symlink refusals, the store lock,
//! claims and briefs, id validation, the `[plan]` model keys and `loom plan summary`.

#[path = "planning_summary_support/mod.rs"]
mod support;

use std::fs;
use std::os::unix::fs::symlink;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use loom::fs::work_dir::PlanConfig;
use loom::planning::runtime::project_key;
use loom::planning::summary::brief::brief;
use loom::planning::summary::claims::{list_claims, release};
use loom::planning::summary::config::SummaryModels;
use loom::planning::summary::lock::with_store_lock;
use loom::user_config::UserConfig;
use serde_json::Value;
use serial_test::serial;
use support::*;
use tempfile::TempDir;

#[test]
fn seal_refuses_symlinked_store() {
    let (t, root, home) = plan_x();
    let outside = t.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(root.join("doc/loom/summaries")).unwrap();
    let store_plans = root.join("doc/loom/summaries/plans");
    symlink(&outside, &store_plans).unwrap();
    let _ = seal_draft(&root, &home, "plans/x", DRAFT_X);
    let count = || fs::read_dir(&outside).unwrap().count();
    assert_eq!(count(), 0, "seal wrote through a symlinked store directory");
    fs::remove_file(&store_plans).unwrap();
    fs::create_dir_all(&store_plans).unwrap();
    let target = outside.join("x.md");
    fs::write(&target, "outside\n").unwrap();
    symlink(&target, root.join(summary_rel("plans/x"))).unwrap();
    let _ = seal_draft(&root, &home, "plans/x", DRAFT_X);
    let content = fs::read_to_string(&target).unwrap();
    assert_eq!(
        content, "outside\n",
        "seal wrote through a symlinked summary file"
    );
    assert_eq!(count(), 1, "the outside directory gained a file");
}

#[test]
fn seal_refuses_symlinked_draft() {
    let (t, root, home) = plan_x();
    let outside = t.path().join("outside.md");
    fs::write(&outside, DRAFT_X).unwrap();
    let draft = paths(&root, &home).drafts.join("plans/x.md");
    fs::create_dir_all(draft.parent().unwrap()).unwrap();
    symlink(&outside, &draft).unwrap();
    let result = seal_id(&root, &home, "plans/x", false);
    assert!(result.is_err(), "sealed a symlinked draft");
    assert!(
        !root.join(summary_rel("plans/x")).exists(),
        "a summary was written"
    );
}

#[test]
fn seal_serialised_by_lock() {
    let (_t, root, home) = plan_x();
    write_draft(&root, &home, "plans/x", DRAFT_X);
    let held = paths(&root, &home);
    let (tx, rx) = mpsc::channel();
    let (released, returned) = std::thread::scope(|s| {
        let holder = s.spawn(move || {
            let hold = with_store_lock(&held, || {
                tx.send(()).unwrap();
                std::thread::sleep(Duration::from_millis(500));
                Ok::<_, anyhow::Error>(Instant::now())
            });
            hold.expect("hold the store lock")
        });
        rx.recv().unwrap();
        let result = seal_id(&root, &home, "plans/x", false);
        let returned = Instant::now();
        result.expect("seal");
        (holder.join().unwrap(), returned)
    });
    assert!(
        returned >= released,
        "seal returned before the lock holder released"
    );
}

#[test]
fn brief_refuses_claimed_summary() {
    let (_t, root, home) = plan_x();
    let p = paths(&root, &home);
    let ids = vec!["plans/x".to_string()];
    brief(&root, &p, &ids, None).expect("first brief");
    let first = list_claims(&p).unwrap();
    assert_eq!(first.len(), 1, "one claim");
    assert_eq!(first[0].summary, "plans/x");
    let err = brief(&root, &p, &ids, None).expect_err("second brief of a claimed summary");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("claim"),
        "refusal does not name the claim: {msg}"
    );
    let again = list_claims(&p).unwrap();
    assert_eq!(again.len(), 1, "a second claim was created");
    assert_eq!(
        again[0].created_at, first[0].created_at,
        "the first claim was overwritten"
    );
    release(&p, &ids).expect("release");
    assert!(
        list_claims(&p).unwrap().is_empty(),
        "release left the claim"
    );
    brief(&root, &p, &ids, None).expect("brief after release");
    write_draft(&root, &home, "plans/x", DRAFT_X);
    seal_id(&root, &home, "plans/x", true).expect("seal");
    assert!(
        list_claims(&p).unwrap().is_empty(),
        "seal left the claim behind"
    );
}

#[test]
fn brief_prints_note_and_marks() {
    let source = "# Sec\u{1b}[31mtion 01\n\nIntro.\n\n## A\n\nAlpha.\n\n## B\n\nBeta.\n";
    let (_t, root, home) = project(&[(S1, source)]);
    let draft = doc_draft(1, &[("Alpha holds.", "a"), ("Beta holds.", "b")]);
    sealed(&root, &home, ID, &draft);
    put(&root, S1, source.replace("Alpha.", "Alpha edited."));
    let p = paths(&root, &home);
    let note = "Alpha holds. needs the new wording";
    let out = brief(&root, &p, &[ID.to_string()], Some(note)).expect("brief");
    assert!(!out.contains('\u{1b}'), "the ESC byte was printed");
    let lines: Vec<&str> = out.lines().collect();
    let (stale, keep) = (marked(&out, "[stale]"), marked(&out, "[keep]"));
    assert_eq!(stale.len(), 1, "one [stale] line:\n{out}");
    assert_eq!(keep.len(), 1, "one [keep] line:\n{out}");
    assert!(lines[stale[0]].contains("Alpha holds."), "{out}");
    assert!(lines[keep[0]].contains("Beta holds."), "{out}");
    let at = lines
        .iter()
        .position(|l| l.contains(note))
        .expect("the note is printed");
    assert!(
        at.abs_diff(stale[0]) <= 1,
        "the note is not beside its sentence:\n{out}"
    );
    let draft = p.drafts.join("demo/product/section_01.md");
    assert!(
        out.contains(&draft.display().to_string()),
        "no draft path:\n{out}"
    );
    let command = "loom plan summary seal demo/product/section_01";
    assert!(out.contains(command), "no seal command:\n{out}");
}

#[test]
fn reseal_produces_no_diff() {
    let (_t, root, home) = project(&[(S1, &section(1, "Alpha.", "Beta."))]);
    let (a, b) = (doc_link(1, "a"), doc_link(1, "b"));
    let preamble = "../../../../product/demo/section_01.md";
    let draft =
        format!("# S1\n\nAlpha holds. [§]({b}) [and]({a})\n\nIntro holds. [§]({preamble})\n");
    let mut files = Vec::new();
    for _ in 0..2 {
        let path = write_draft(&root, &home, ID, &draft);
        seal_id(&root, &home, ID, false).expect("seal");
        assert!(!path.exists(), "the draft was left behind");
        files.push(read(&root, &summary_rel(ID)));
    }
    assert_eq!(files[0], files[1], "a reseal changed the summary file");
    let front = files[0].split("\n---\n").next().unwrap();
    assert!(
        !front.contains("model"),
        "front matter names a model:\n{front}"
    );
    let at = |needle: &str| {
        front
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle}:\n{front}"))
    };
    let order = [at("kind:"), at("of:"), at("file:"), at("cites:")];
    assert!(
        order.windows(2).all(|w| w[0] < w[1]),
        "front matter order:\n{front}"
    );
    let sorted = at("section_01.md#a") < at("section_01.md#b");
    assert!(sorted, "cites unordered:\n{front}");
}

#[test]
fn summary_id_rejects_traversal() {
    let s1 = section(1, "Alpha.", "Beta.");
    let (_t, root, home) = project(&[(S1, &s1), (PLAN_X, "# Plan: X\n")]);
    let p = paths(&root, &home);
    let ids = [
        "../x",
        "plans/../../etc",
        "plans/",
        "plans/a/b",
        "/product",
        "demo/product/section_1x",
    ];
    for id in ids {
        let err = seal_id(&root, &home, id, true).expect_err(id);
        assert!(
            err.contains(id),
            "seal {id}: error does not name the id: {err}"
        );
        let err = format!(
            "{:#}",
            brief(&root, &p, &[id.to_string()], None).expect_err(id)
        );
        assert!(
            err.contains(id),
            "brief {id}: error does not name the id: {err}"
        );
    }
    let claims = fs::read_dir(&p.claims).map(|d| d.count()).unwrap_or(0);
    assert_eq!(claims, 0, "a claim was created for an invalid id");
    assert!(!root.join("doc/loom").exists(), "a store path was created");
}

#[test]
#[serial]
fn plan_summary_keys_resolve() {
    let home = TempDir::new().unwrap();
    let user_toml = "[plan]\ndocument_summary_model = \"sonnet\"\noverview_model = \"haiku\"\n";
    fs::write(home.path().join("config.toml"), user_toml).unwrap();
    let user = with_loom_home(home.path(), UserConfig::load);
    let project = PlanConfig::from_section_toml("overview_model = \"opus\"").expect("planning");
    let resolved = SummaryModels::resolve(&project, &user);
    assert_eq!(models(&resolved), ["sonnet", "medium", "opus", "high"]);
    let empty = TempDir::new().unwrap();
    let user = with_loom_home(empty.path(), UserConfig::load);
    let project = PlanConfig::from_section_toml("").expect("empty planning table");
    let defaults = SummaryModels::resolve(&project, &user);
    assert_eq!(models(&defaults), ["haiku", "high", "haiku", "high"]);
}

#[test]
#[serial]
fn plan_keys_beside_run_identity() {
    let tmp = TempDir::new().unwrap();
    let run =
        "[plan]\nsource_path = \"doc/plans/demo/PLAN-a.md\"\nplan_id = \"a\"\nplan_name = \"A\"\n";
    let config = format!("{run}base_branch = \"main\"\n\n[planning]\noverview_model = \"opus\"\n");
    put(tmp.path(), ".loom/work/config.toml", config);
    let empty = TempDir::new().unwrap();
    let loaded = with_loom_home(empty.path(), || SummaryModels::load(tmp.path()));
    let loaded = loaded.expect("load beside the run's [plan] table");
    assert_eq!(models(&loaded), ["haiku", "medium", "opus", "high"]);
}

#[test]
fn seal_refuses_foreign_claim() {
    let (_t, root, home) = git_plan_x();
    let summary = root.join(summary_rel("plans/x"));
    let seal = |force: bool, envs: &[(&str, &str)]| {
        let args: &[&str] = if force {
            &["seal", "plans/x", "--force"]
        } else {
            &["seal", "plans/x"]
        };
        loom_summary(&root, &home, args, envs)
    };
    let claim = plant_claim(&root, &home);
    let out = seal(false, &[("LOOM_PLAN_SESSION", "s2")]);
    assert_eq!(out.status.code(), Some(1), "s2 sealed: {}", stderr(&out));
    assert!(
        stderr(&out).contains("s1"),
        "the holder is not named: {}",
        stderr(&out)
    );
    assert!(
        claim.exists() && !summary.exists(),
        "s2 touched the claim or wrote"
    );
    let out = seal(false, &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a terminal sealed without --force"
    );
    assert!(
        claim.exists() && !summary.exists(),
        "the terminal touched the claim or wrote"
    );
    let out = seal(true, &[]);
    assert!(out.status.success(), "--force refused: {}", stderr(&out));
    assert!(summary.exists(), "--force wrote nothing");
    plant_claim(&root, &home);
    let out = seal(false, &[("LOOM_PLAN_SESSION", "s1")]);
    assert!(out.status.success(), "claimant refused: {}", stderr(&out));
    assert!(!claim.exists(), "the claimant's seal left its claim");
}

#[test]
fn summary_cli_uses_session_state() {
    let (t, root, home) = git_project(&[(S1, &section(1, "Alpha.", "Beta."))]);
    let elsewhere = t.path().join("elsewhere");
    let state = elsewhere.join(project_key(&root).unwrap());
    let draft = state.join("drafts/demo/product/section_01.md");
    fs::create_dir_all(draft.parent().unwrap()).unwrap();
    chmod(&state, 0o700);
    let body = doc_draft(1, &[("Alpha holds.", "a")]);
    fs::write(&draft, &body).unwrap();
    let envs = [
        ("LOOM_PLAN_SESSION", UUID),
        ("LOOM_PLAN_STATE", state.to_str().unwrap()),
    ];
    chmod(&elsewhere, 0o555);
    let out = loom_summary(&root, &home, &["seal", ID], &envs);
    chmod(&elsewhere, 0o755);
    assert!(out.status.success(), "session seal: {}", stderr(&out));
    let summary = root.join(summary_rel(ID));
    assert!(summary.exists(), "the session draft was not sealed");
    assert!(
        !home.join("plan").exists(),
        "the CLI used LOOM_HOME inside a session"
    );
    fs::remove_file(&summary).unwrap();
    fs::write(&draft, &body).unwrap();
    let out = loom_summary(&root, &home, &["seal", ID], &envs);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a writable parent was trusted: {}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("LOOM_PLAN_STATE"), "{}", stderr(&out));
    assert!(!summary.exists(), "sealed under a writable parent");
}

#[test]
fn summary_cli_status_json() {
    let (_t, root, home) = git_project(&[(VISION, &vision(&[1])), (S1, &section(1, "A.", "B."))]);
    sealed(&root, &home, ID, &doc_draft(1, &[("Alpha holds.", "a")]));
    let out = loom_summary(&root, &home, &["status", "--json"], &[]);
    assert!(out.status.success(), "status failed: {}", stderr(&out));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{e}: {stdout}"));
    assert_eq!(v["schema"], 1);
    let summaries = v["summaries"].as_array().expect("summaries array");
    let ids: Vec<&str> = summaries.iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(ids, [ID, PRODUCT]);
    for s in summaries {
        assert_keys(
            s,
            &[
                "id",
                "state",
                "source",
                "path",
                "sentences",
                "claim",
                "error",
            ],
        );
        assert!(s["claim"].is_null(), "{s}");
    }
    assert_eq!(summaries[0]["state"], "current");
    assert_eq!(summaries[1]["state"], "missing");
    let sentence = &summaries[0]["sentences"][0];
    assert_keys(sentence, &["id", "line", "state", "text"]);
    assert_eq!(sentence["id"], sid("Alpha holds."));
    assert_eq!(sentence["text"], "Alpha holds.");
    assert_eq!(sentence["state"], "current");
}

#[test]
fn summary_cli_seal_reports_line() {
    let (_t, root, home) = git_project(&[(S1, &section(1, "Alpha.", "Beta."))]);
    let l = doc_link(1, "a");
    let body = format!("# S1\n\nAlpha holds. [§]({l})\n- item [§]({l})\n");
    let draft = write_draft(&root, &home, ID, &body);
    let out = loom_summary(&root, &home, &["seal", ID], &[]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a rejected seal: {}",
        stderr(&out)
    );
    let want = format!("{}:4:", draft.display());
    assert!(
        stderr(&out).contains(&want),
        "stderr lacks {want}: {}",
        stderr(&out)
    );
    assert!(!root.join(summary_rel(ID)).exists());
}
