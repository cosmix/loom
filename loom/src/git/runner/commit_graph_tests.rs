//! The runner reads commits from their objects, never from a commit-graph
//! file a session could forge.

use super::run_git;
use crate::test_git::{git, git_unchecked, TestRepo};
use std::path::Path;

/// A commit-graph parent slot that names no parent (`GRAPH_PARENT_NONE`).
const NO_PARENT: u32 = 0x7000_0000;
/// Bytes per commit in the `CDAT` chunk: tree id, two parent slots, then
/// generation and commit time.
const CDAT_WIDTH: usize = 20 + 4 + 4 + 8;
/// Bytes per object id in a SHA-1 repository.
const OID_WIDTH: usize = 20;

fn commit(root: &Path, message: &str) -> String {
    git(root, &["commit", "--allow-empty", "-m", message]);
    git(root, &["rev-parse", "HEAD"])
}

fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().unwrap())
}

/// The offset of chunk `id` in a commit-graph file: the header is 8 bytes,
/// its seventh the chunk count, then 12-byte entries of id and offset.
fn chunk(graph: &[u8], id: &[u8; 4]) -> usize {
    (0..usize::from(graph[6]))
        .map(|index| 8 + 12 * index)
        .find(|&entry| &graph[entry..entry + 4] == id)
        .map(|entry| u64::from_be_bytes(graph[entry + 4..entry + 12].try_into().unwrap()))
        .map(|offset| usize::try_from(offset).unwrap())
        .unwrap_or_else(|| panic!("no {} chunk", String::from_utf8_lossy(id)))
}

/// Rewrite the repository's commit-graph so it records `commit` as having
/// no parent.
fn forge_root_commit(root: &Path, commit: &str) {
    let path = root.join(".git/objects/info/commit-graph");
    let mut graph = std::fs::read(&path).unwrap();
    let oid: Vec<u8> = (0..commit.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&commit[at..at + 2], 16).unwrap())
        .collect();
    let fanout = chunk(&graph, b"OIDF");
    let count = be_u32(&graph[fanout + 255 * 4..fanout + 256 * 4]) as usize;
    let ids = chunk(&graph, b"OIDL");
    let position = graph[ids..ids + OID_WIDTH * count]
        .chunks(OID_WIDTH)
        .position(|id| id == oid.as_slice())
        .expect("the commit is in the graph");
    let slot = chunk(&graph, b"CDAT") + CDAT_WIDTH * position + OID_WIDTH;
    graph[slot..slot + 4].copy_from_slice(&NO_PARENT.to_be_bytes());
    // git writes the file read-only; replace it.
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, graph).unwrap();
}

#[test]
fn a_forged_commit_graph_does_not_change_ancestry() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    TestRepo::builder().no_seed().init_in(root);
    let base = commit(root, "base");
    let middle = commit(root, "middle");
    let tip = commit(root, "tip");
    git(root, &["commit-graph", "write", "--reachable"]);
    // git parses a commit named on its command line from its object, and a
    // commit it reaches in a walk from the graph: the forged one is between.
    forge_root_commit(root, &middle);
    let ancestry = ["merge-base", "--is-ancestor", base.as_str(), tip.as_str()];

    // Positive control: plain git trusts the forged graph.
    let plain = git_unchecked(root, &ancestry);
    assert_eq!(
        plain.status.code(),
        Some(1),
        "the forged graph must be live"
    );

    let output = run_git(&ancestry, root).unwrap();
    assert!(output.status.success(), "{output:?}");
}
