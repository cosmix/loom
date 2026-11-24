//! Cycle detection on long dependency chains.

use super::*;
use crate::plan::schema::StageDefinition;

fn make_stage(id: &str, deps: Vec<&str>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        dependencies: deps.into_iter().map(String::from).collect(),
        working_dir: ".".to_string(),
        ..Default::default()
    }
}

/// Cycle detection must not recurse with the chain length: a 100,000 stage
/// chain builds on a 256 KiB thread stack, and closing it into a ring reports
/// the cycle.
#[test]
fn test_long_dependency_chain_does_not_overflow_the_stack() {
    const LENGTH: usize = 100_000;
    let chain = |ring: bool| -> Vec<StageDefinition> {
        (0..LENGTH)
            .map(|index| {
                let dependency = match index {
                    0 if ring => Some(LENGTH - 1),
                    0 => None,
                    _ => Some(index - 1),
                };
                let dependency = dependency.map(|d| format!("s{d}"));
                make_stage(
                    &format!("s{index}"),
                    dependency.iter().map(String::as_str).collect(),
                )
            })
            .collect()
    };

    let outcome = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let built = ExecutionGraph::build(chain(false)).map(|graph| graph.nodes.len());
            let ring = ExecutionGraph::build(chain(true)).map(|_| ());
            (
                built.map_err(|e| e.to_string()),
                ring.map_err(|e| e.to_string()),
            )
        })
        .unwrap()
        .join()
        .unwrap();

    assert_eq!(outcome.0, Ok(LENGTH));
    assert!(outcome
        .1
        .unwrap_err()
        .starts_with("Circular dependency detected: s0 -> s99999"));
}
