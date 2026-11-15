use super::{declared_input, named_inputs};
use crate::models::stage::Stage;

fn stage(working_dir: Option<&str>, files: &[&str]) -> Stage {
    Stage {
        working_dir: working_dir.map(str::to_owned),
        files: files.iter().map(|file| (*file).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn declaration_climbing_out_of_the_repository_names_nothing_inside() {
    let stage = stage(None, &["../outside.rs"]);

    assert!(!declared_input(&stage, "outside.rs").unwrap());
    assert!(named_inputs(&stage, "outside.rs").unwrap().is_empty());
}

#[test]
fn declaration_climbing_out_of_working_dir_stays_inside_the_repository() {
    let stage = stage(Some("apps/api"), &["../shared.rs"]);

    assert!(declared_input(&stage, "apps/shared.rs").unwrap());
    assert!(!declared_input(&stage, "shared.rs").unwrap());
}
