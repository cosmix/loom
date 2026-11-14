use super::*;
use crate::planning::guidance::{opening_message, AuthoringStart};
use std::fs;
use tempfile::TempDir;

const LEDGER: &str = "doc/plans/f/findings.md";

fn verified(id: &str, about: &str, quote: &str, status: &str) -> String {
    format!(
        "## {id} Finding {id}\n\nAbout: {about}\nEvidence: {quote}\nSeverity: high\n\
         Status: {status}\n\nWhat is wrong in {id}.\n\n"
    )
}

fn fixture(ledger: &str) -> (TempDir, Project) {
    let temp = tempfile::tempdir().unwrap();
    let files = [
        (
            "doc/product/f/index.md",
            "# Vision\n\n## Specification sections\n\n\
             | # | Section |\n| --- | --- |\n| 01 | One |\n| 02 | Two |\n",
        ),
        (
            "doc/product/f/section_01.md",
            "# One\n\n## Budget\n\nThe budget counts plans.\n",
        ),
        (
            "doc/product/f/section_02.md",
            "# Two\n\nThe second section says this.\n",
        ),
        (
            "doc/plans/f/index.md",
            "# Metaplan\n\n## Done (`PLAN-done.md`)\n",
        ),
        (
            "doc/plans/f/DONE-PLAN-done.md",
            "# Done\n\nThe done plan built it.\n",
        ),
        (LEDGER, ledger),
    ];
    for (path, text) in files {
        let path = temp.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    let project = Project::load(temp.path()).unwrap();
    (temp, project)
}

fn ledger() -> String {
    let entries = [
        verified("F-1", "section_01#budget", "budget counts plans", "open"),
        verified(
            "F-2",
            "section_02, PLAN-done.md",
            "second section says this",
            "open",
        ),
        verified("F-3", "PLAN-done.md", "done plan built it", "open"),
        "## F-4 Raw\n\nAbout: section_01\nStatus: open\n\nUnverified.\n\n".to_owned(),
        verified(
            "F-5",
            "section_02",
            "The second section says",
            "dismissed: no",
        ),
    ];
    format!("# Findings\n\n{}", entries.concat())
}

#[test]
fn the_run_takes_actionable_findings_and_their_editable_documents() {
    let (_temp, project) = fixture(&ledger());
    let run = address_run(&project, "f").unwrap();
    let ids: Vec<&str> = run.findings.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, ["F-1", "F-2", "F-3"]);
    let one = PathBuf::from("doc/product/f/section_01.md");
    let two = PathBuf::from("doc/product/f/section_02.md");
    assert_eq!(
        run.findings[1].documents,
        std::slice::from_ref(&two),
        "a done plan is locked"
    );
    assert!(run.findings[2].documents.is_empty());
    assert_eq!(run.documents, [one, two, PathBuf::from(LEDGER)]);
    assert_eq!(run.teammates, 2);
}

#[test]
fn the_team_scope_holds_every_document_the_opening_names() {
    let (_temp, project) = fixture(&ledger());
    let start = AuthoringStart::Offer(Offer::AddressFindings);
    let opening = opening_message(&start, &project, Some("f")).unwrap();
    let scope = crate::planning::guidance::team_scope(&project, "address-findings", "f")
        .unwrap()
        .unwrap();
    let crate::plan_session::scope::SessionScope::Team { run, documents, .. } = scope else {
        panic!("not a team scope");
    };
    let ledger = PathBuf::from(LEDGER);
    assert_eq!(
        run,
        crate::plan_session::scope::TeamRun::Address {
            ledger: ledger.clone()
        }
    );
    assert_eq!(documents, address_run(&project, "f").unwrap().documents);
    for path in [
        "doc/product/f/section_01.md",
        "doc/product/f/section_02.md",
        LEDGER,
    ] {
        assert!(opening.contains(path), "{opening}");
        assert!(documents.contains(&PathBuf::from(path)), "{path}");
    }
}

#[test]
fn the_offer_starts_only_with_an_actionable_finding() {
    let (_temp, project) = fixture(&ledger());
    let start = AuthoringStart::from_offer_id("address-findings", &project, Some("f")).unwrap();
    assert_eq!(start, AuthoringStart::Offer(Offer::AddressFindings));
    let opening = opening_message(&start, &project, Some("f")).unwrap();
    assert!(opening.contains("loom-plan:address-findings"), "{opening}");
    assert!(
        opening.contains("F-1 (doc/product/f/section_01.md)"),
        "{opening}"
    );
    assert!(opening.contains("F-3 (no editable document)"), "{opening}");
    assert!(opening.contains("with 2 teammates"), "{opening}");
    assert!(!opening.contains("F-4"), "{opening}");

    let (_temp, bare) = fixture("# Findings\n\n## F-1 Raw\n\nStatus: open\n\nNo quote.\n");
    let refused = AuthoringStart::from_offer_id("address-findings", &bare, Some("f"));
    assert_eq!(
        refused,
        Err(StartError::NotOffered("address-findings".into()))
    );
    let unnamed = AuthoringStart::from_offer_id("address-findings", &project, None);
    assert_eq!(unnamed, Err(StartError::FeatureRequired));
}
