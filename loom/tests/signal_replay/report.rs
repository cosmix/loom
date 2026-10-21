//! Assembles the `signal-replay v1` report and checks its shape.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use loom::context::delivery::DeliveryRecord;
use serde_json::Value;

use super::{input, render};

pub const START: &str = "=== signal-replay v1 ===";
pub const END: &str = "=== end signal-replay ===";
pub const HEADER: &str =
    "signal-replay v1 plan=PLAN-token-flow estimate=bytes-times-recorded-requests";

/// A rendered report plus the evidence the self-checks need.
pub struct Report {
    pub lines: Vec<String>,
    /// `(stage/query id, envelope bytes)` for every worker query.
    pub envelopes: Vec<(String, usize)>,
    /// Worker delivery records with a non-empty `delivered` list.
    pub delivering_records: usize,
}

/// Value of `key=<n>` in a tab-separated record.
pub fn field(line: &str, key: &str) -> Option<u64> {
    line.split('\t')
        .find_map(|f| f.strip_prefix(key)?.strip_prefix('=')?.parse().ok())
}

fn asset_paths() -> Result<Vec<PathBuf>> {
    let root = input::repo_root();
    let mut paths = vec![
        PathBuf::from("CLAUDE.md.template"),
        PathBuf::from("loom-hooks/_subagent-preamble.txt"),
    ];
    for entry in fs::read_dir(root.join("agents"))? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            paths.push(Path::new("agents").join(path.file_name().context("agent name")?));
        }
    }
    for entry in fs::read_dir(root.join("skills"))? {
        let dir = entry?.path();
        if dir.join("SKILL.md").is_file() {
            let name = dir.file_name().context("skill dir")?;
            paths.push(Path::new("skills").join(name).join("SKILL.md"));
        }
    }
    // `check_shape` compares the rendered strings; `PathBuf` order is
    // component-wise and puts `skills/a/` before `skills/a-b/`, the string
    // order does not.
    paths.sort_by_cached_key(|path| path.display().to_string());
    Ok(paths)
}

fn asset_lines() -> Result<Vec<String>> {
    let root = input::repo_root();
    asset_paths()?
        .iter()
        .map(|p| {
            let bytes = fs::metadata(root.join(p))
                .with_context(|| p.display().to_string())?
                .len();
            Ok(format!("asset\t{}\tbytes={bytes}", p.display()))
        })
        .collect()
}

fn count_delivering(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| {
            let path = e.path();
            if path.is_dir() {
                return count_delivering(&path);
            }
            let in_worker_dir = path
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|n| n == "worker-brief");
            if !in_worker_dir || path.extension().and_then(|x| x.to_str()) != Some("json") {
                return 0;
            }
            let record = fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<DeliveryRecord>(&raw).ok());
            usize::from(record.is_some_and(|r| !r.delivered.is_empty()))
        })
        .sum()
}

fn sum_of(lines: &[String], kind: &str, key: &str) -> u64 {
    let prefix = format!("{kind}\t");
    lines
        .iter()
        .filter(|l| l.starts_with(&prefix))
        .filter_map(|l| field(l, key))
        .sum()
}

fn total_line(lines: &[String]) -> String {
    let resident =
        sum_of(lines, "signal", "resident_estimate") + sum_of(lines, "worker", "resident_estimate");
    format!(
        "total\tsignal_bytes={}\tfile_bytes={}\tbrief_bytes={}\tasset_bytes={}\tresident_estimate={resident}",
        sum_of(lines, "signal", "total"),
        sum_of(lines, "signal", "file"),
        sum_of(lines, "worker", "brief"),
        sum_of(lines, "asset", "bytes"),
    )
}

/// Render the whole report inside the fresh root `root`.
pub fn render(root: &Path) -> Result<Report> {
    let requests: Value = serde_json::from_str(&fs::read_to_string(
        input::fixture_dir().join("requests.json"),
    )?)?;
    let (work_dir, stages) = input::build(root)?;
    let mut lines = vec![START.to_string(), HEADER.to_string()];
    lines.extend(render::signal_lines(&work_dir, &stages, &requests)?);

    let home = root.join("replay-home");
    fs::create_dir_all(&home)?;
    fs::write(home.join("config.toml"), "[update]\ncheck = false\n")?;
    let worker_requests = render::request_count(&requests, &["worker"])?;
    let mut envelopes = Vec::new();
    for q in render::queries(&stages)? {
        let issued = render::issue(root, &home, &q)?;
        envelopes.push((format!("{}/{}", q.stage_id, q.id), issued.envelope_bytes));
        lines.push(format!(
            "worker\t{}\t{}\t{}\tquery={}\tbrief={}\tenvelope={}\trequests={worker_requests}\tresident_estimate={}",
            q.stage_id,
            q.id,
            q.subagent_type,
            issued.query_bytes,
            issued.brief_bytes,
            issued.envelope_bytes,
            issued.brief_bytes as u64 * worker_requests
        ));
    }
    lines.extend(asset_lines()?);
    let totals = total_line(&lines[2..]);
    lines.push(totals);
    lines.push(END.to_string());
    Ok(Report {
        lines,
        envelopes,
        delivering_records: count_delivering(&work_dir),
    })
}

fn require_keys(line: &str, keys: &[&str]) -> Result<()> {
    for key in keys {
        if field(line, key).is_none() {
            bail!("record lacks {key}: {line}");
        }
    }
    Ok(())
}

const SIGNAL_KEYS: [&str; 8] = [
    "stable_prefix",
    "semi_stable",
    "dynamic",
    "recitation",
    "total",
    "file",
    "requests",
    "resident_estimate",
];
const WORKER_KEYS: [&str; 5] = [
    "query",
    "brief",
    "envelope",
    "requests",
    "resident_estimate",
];

/// Shape checks of self-check 4 (markers, header, record kinds, totals).
pub fn check_shape(lines: &[String], stage_count: usize) -> Result<()> {
    if lines.first().map(String::as_str) != Some(START)
        || lines.last().map(String::as_str) != Some(END)
    {
        bail!("report is not framed by its markers");
    }
    if lines.get(1).map(String::as_str) != Some(HEADER) {
        bail!("second line is not the header");
    }
    let body = &lines[2..lines.len() - 1];
    let (total, records) = body.split_last().context("empty report")?;
    let (mut signals, mut skipped, mut last_asset) = (0, 0, String::new());
    for line in records {
        let cells: Vec<&str> = line.split('\t').collect();
        match cells[0] {
            "signal" if line.contains("skipped=knowledge-signal") => {
                signals += 1;
                skipped += 1;
            }
            "signal" => {
                signals += 1;
                require_keys(line, &SIGNAL_KEYS)?;
            }
            "worker" => require_keys(line, &WORKER_KEYS)?,
            "asset" => {
                require_keys(line, &["bytes"])?;
                if field(line, "bytes") == Some(0) || cells[1] <= last_asset.as_str() {
                    bail!("asset line is empty or unsorted: {line}");
                }
                last_asset = cells[1].to_string();
            }
            other => bail!("unknown record kind {other}"),
        }
    }
    if signals != stage_count || skipped != 1 {
        bail!("expected {stage_count} signal lines with 1 skipped, got {signals} and {skipped}");
    }
    if *total != total_line(records) {
        bail!("total line disagrees with the records above it: {total}");
    }
    Ok(())
}

/// Self-check 2: per-section sums on every rendered signal.
pub fn check_sections(lines: &[String]) -> Result<()> {
    for line in lines
        .iter()
        .filter(|l| l.starts_with("signal\t") && !l.contains("skipped="))
    {
        let f = |k| field(line, k).unwrap_or(0);
        let sum = f("stable_prefix") + f("semi_stable") + f("dynamic") + f("recitation");
        if f("stable_prefix") == 0 || f("total") != sum {
            bail!("section sizes disagree: {line}");
        }
    }
    Ok(())
}
