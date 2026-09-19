//! The checked-in abstention cases, retrieved end to end over a small knowledge
//! tree whose sections share words with them: both must leave the prompt hook
//! silent, and a prompt naming a section must still reach it.

use super::cases::{load_cases_file, CASES_RELATIVE_PATH};
use crate::commands::hook::user_prompt::delivered;
use crate::context::config::RetrievalConfig;
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Each section shares several words with the two abstention prompts in its
/// BODY, and at most one with either of them in its heading.
const CONVENTIONS: &str = "# Conventions\n\n\
    ## Version and release identity\n\n\
    The installed binary reports the repository version it was built from.\n\
    Installed copies of the hooks carry the same version as the repository files.\n\
    Nothing else is stamped at install time; that is all the identity there is.\n\n\
    ## Files and scope\n\n\
    Files owned by a stage are listed in its brief, and those files are the scope.\n\
    Use the repository copies of shared files, not the installed copies.\n\
    Thanks to the scope list, nothing else needs reading for now.\n";

const MISTAKES: &str = "# Mistakes\n\n\
    ## Phantom merges from defensive assume-merged branches\n\n\
    A stage was marked merged but its branch never actually merged.\n\
    The lesson: verify the merge before marking the stage merged.\n\n\
    ## Why this class is hard to see\n\n\
    Nothing in the output looks wrong, so there is nothing else to do here\n\
    until the class is named; that is all it takes for now.\n";

fn project() -> TempDir {
    let temp = TempDir::new().unwrap();
    let knowledge = temp.path().join("doc/loom/knowledge");
    fs::create_dir_all(temp.path().join(".loom/work")).unwrap();
    fs::create_dir_all(&knowledge).unwrap();
    fs::write(knowledge.join("conventions.md"), CONVENTIONS).unwrap();
    fs::write(knowledge.join("mistakes.md"), MISTAKES).unwrap();
    temp
}

/// The query of the checked-in case called `name`.
fn case_query(name: &str) -> String {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let cases = load_cases_file(&checkout.join(CASES_RELATIVE_PATH)).unwrap();
    cases
        .cases
        .into_iter()
        .find(|case| case.name == name)
        .unwrap_or_else(|| panic!("no eval case named {name}"))
        .query
}

fn emits(root: &Path, case: &str) -> bool {
    let config = RetrievalConfig::default();
    let query = StageQuery::new(root, case_query(case));
    let pack = retrieve_for_stage(&query, config.prompt_budget_tokens).unwrap();
    assert!(
        !pack.items.is_empty(),
        "{case} must retrieve something to judge"
    );
    delivered(&pack, &config).is_some()
}

#[test]
fn the_conversational_correction_abstains() {
    let project = project();
    assert!(!emits(project.path(), "conversational-correction-abstains"));
}

#[test]
fn the_farewell_abstains() {
    let project = project();
    assert!(!emits(project.path(), "farewell-abstains"));
}

#[test]
fn a_prompt_naming_a_section_still_emits() {
    let project = project();
    assert!(emits(project.path(), "genuine-win-phantom-merges"));
}
