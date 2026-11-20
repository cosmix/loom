//! Contract `contracts-and-criteria-selections-do-not-mix`: `check_selection`
//! keeps a contracts run, a replacement trial and a criterion subset apart and
//! refuses a mix instead of silently dropping one of them.
use crate::commands::verify::{check_selection, CheckArgs, CheckSelection};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    args: CheckArgs,
}

fn args(flags: &[&str]) -> CheckArgs {
    let argv = std::iter::once("loom-check").chain(flags.iter().copied());
    Cli::try_parse_from(argv).expect("the flags parse").args
}

#[test]
fn contracts_and_criteria_selections_do_not_mix() {
    let contracts = check_selection(&args(&["--contracts"])).expect("--contracts alone");
    assert!(
        matches!(contracts, CheckSelection::Contracts),
        "--contracts alone selects the contracts run"
    );

    assert!(
        check_selection(&args(&["--contracts", "--criterion", "0"])).is_err(),
        "--contracts with --criterion must be refused"
    );
    assert!(
        check_selection(&args(&["--replacement", "src/t.rs=/scratch/t.rs"])).is_err(),
        "--replacement without --contracts must be refused"
    );

    let trial = check_selection(&args(&[
        "--contracts",
        "--replacement",
        "src/t.rs=/scratch/t.rs",
    ]))
    .expect("--contracts with a replacement");
    match trial {
        CheckSelection::Trial(files) => assert_eq!(
            files,
            vec![("src/t.rs".to_string(), PathBuf::from("/scratch/t.rs"))]
        ),
        _ => panic!("a replacement must select a trial, not an ordinary check"),
    }
}
