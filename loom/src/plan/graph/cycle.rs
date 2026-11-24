//! Cycle detection algorithms for the execution graph

use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

use super::nodes::StageNode;

/// Detect circular dependencies in the graph using a depth-first search.
///
/// The search keeps its own stack, so a long dependency chain cannot overflow
/// the thread stack the way recursion would.
pub fn detect_cycles(nodes: &HashMap<String, StageNode>) -> Result<()> {
    let mut visited: HashSet<&str> = HashSet::new();

    // Sorted so the reported cycle is the same on every run.
    let mut ids: Vec<&String> = nodes.keys().collect();
    ids.sort();
    for node_id in ids {
        if !visited.contains(node_id.as_str()) {
            if let Some(cycle) = find_cycle_from(nodes, node_id, &mut visited) {
                bail!("Circular dependency detected: {}", cycle.join(" -> "));
            }
        }
    }

    Ok(())
}

/// One node on the search path and the index of its next dependency to try.
struct Frame<'a> {
    id: &'a str,
    next_dep: usize,
}

/// Depth-first search from `start`. The frames on `path` are the nodes of the
/// current path; a dependency that is already visited and still on the path
/// closes a cycle, reported from its first occurrence on the path to the
/// dependency again. `on_path` maps each path node to its index in `path`, so
/// the check per edge stays constant-time.
fn find_cycle_from<'a>(
    nodes: &'a HashMap<String, StageNode>,
    start: &'a str,
    visited: &mut HashSet<&'a str>,
) -> Option<Vec<String>> {
    let mut path = vec![Frame {
        id: start,
        next_dep: 0,
    }];
    let mut on_path: HashMap<&str, usize> = HashMap::from([(start, 0)]);
    visited.insert(start);

    while let Some(frame) = path.last_mut() {
        let dependencies = nodes.get(frame.id).map(|node| node.dependencies.as_slice());
        let Some(dep) = dependencies.and_then(|deps| deps.get(frame.next_dep)) else {
            on_path.remove(frame.id);
            path.pop();
            continue;
        };
        frame.next_dep += 1;
        if visited.insert(dep.as_str()) {
            on_path.insert(dep, path.len());
            path.push(Frame {
                id: dep,
                next_dep: 0,
            });
        } else if let Some(&position) = on_path.get(dep.as_str()) {
            let mut cycle: Vec<String> =
                path[position..].iter().map(|f| f.id.to_string()).collect();
            cycle.push(dep.clone());
            return Some(cycle);
        }
    }
    None
}
