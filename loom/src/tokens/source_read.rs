//! Reading the lines each transcript gained since the checkpoint.

use super::accumulate::AgentState;
use super::anchor;
use super::checkpoint::Checkpoint;
use super::discover::Source;
use super::tail::{self, Line};
use super::tracker::LastLine;
use crate::context::untrusted::inline_safe;
use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Reads every source until `stop` is set. Returns the `(stage id, agent key)`
/// of each agent whose file was read, and the first file that could not be.
pub(super) fn read_sources<'a>(
    checkpoint: &mut Checkpoint,
    sources: &'a [Source],
    stop: &AtomicBool,
    last_line: LastLine,
) -> (HashSet<(&'a str, &'a str)>, Option<String>) {
    let mut advanced = HashSet::new();
    let mut failure = None;
    for source in sources {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        match read_source(checkpoint, source, stop, last_line) {
            Ok(false) => continue,
            Ok(true) => {}
            // A read that failed part way may still have moved the offset.
            Err(error) => {
                failure.get_or_insert_with(|| {
                    format!("cannot read {}: {error}", inline_safe(&source.path))
                });
            }
        }
        advanced.insert((source.stage_id.as_str(), source.key.as_str()));
    }
    (advanced, failure)
}

/// Reads the lines `source`'s file gained since the checkpoint, and says
/// whether there were any. A missing file is skipped and its agent keeps its
/// totals. The agent restarts from offset 0 when the file has a new inode, a
/// different path, shrank below the saved offset, was rewritten in place (the
/// bytes before the saved offset no longer match their saved anchor), or grew
/// after its message-id table was shed to fit the checkpoint cap. A line over
/// [`tail::MAX_LINE_BYTES`] is skipped and counted as unreadable. An unfinished
/// last line is handled as `last_line` says.
fn read_source(
    checkpoint: &mut Checkpoint,
    source: &Source,
    stop: &AtomicBool,
    last_line: LastLine,
) -> std::io::Result<bool> {
    let path = Path::new(&source.path);
    let Some(stat) = tail::stat(path) else {
        return Ok(false);
    };
    let agent = checkpoint.agent(source);
    if source.agent_type.is_some() {
        agent.agent_type.clone_from(&source.agent_type);
    }
    if source.forwarder.is_some() {
        agent.forwarder.clone_from(&source.forwarder);
    }
    if source.unit.is_some() {
        agent.unit.clone_from(&source.unit);
    }
    let file = &agent.file;
    let moved = file.path != source.path || file.inode != stat.inode || stat.len < file.offset;
    let shed_and_grown = file.table_shed && stat.len > file.offset;
    if moved || shed_and_grown || anchor::rewritten(path, file)? {
        agent.restart(&source.path, stat.inode);
    }
    if stat.len == agent.file.offset {
        return Ok(false);
    }
    let mut offset = agent.file.offset;
    let read = tail::read_lines(path, &mut offset, stat.len, stop, |line| match line {
        Line::Text(text) => agent.ingest(text),
        Line::TooLong => agent.count_unreadable(),
    });
    agent.file.offset = offset;
    let counted = match read {
        Ok(()) if last_line == LastLine::Count && !stop.load(Ordering::SeqCst) => {
            count_last_line(agent, path, stat.len)
        }
        other => other,
    };
    agent.file.anchor = anchor::of_prefix(path, agent.file.offset)
        .ok()
        .flatten()
        .unwrap_or(0);
    counted?;
    Ok(true)
}

/// Ingests the unfinished line between the agent's offset and `len` when it
/// is a whole JSON value, and moves the offset past it. A fragment that does
/// not parse is a line still being written: it is left for a later pass.
fn count_last_line(agent: &mut AgentState, path: &Path, len: u64) -> std::io::Result<()> {
    let Some(fragment) = tail::read_fragment(path, agent.file.offset, len)? else {
        return Ok(());
    };
    if serde_json::from_str::<serde::de::IgnoredAny>(&fragment).is_ok() {
        agent.ingest(&fragment);
        agent.file.offset = len;
    }
    Ok(())
}
