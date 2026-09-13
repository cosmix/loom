//! Advisory ownership checks for the optional worker table in a description.

use std::collections::HashMap;
use std::path::{Component, Path};

use super::super::types::StageDefinition;

#[derive(Debug)]
struct WorkerClaim {
    worker: String,
    path: String,
    row: usize,
}

pub(super) fn check_worker_table_ownership(stages: &[StageDefinition]) -> Vec<String> {
    let mut warnings = Vec::new();

    for stage in stages {
        let claims = stage
            .description
            .as_deref()
            .map(worker_claims)
            .unwrap_or_default();
        warnings.extend(duplicate_claim_warnings(&stage.id, &claims));
        warnings.extend(outside_files_warnings(&stage.id, &stage.files, &claims));
    }

    warnings
}

fn worker_claims(description: &str) -> Vec<WorkerClaim> {
    let lines: Vec<_> = description.lines().collect();
    let mut in_code_block = false;

    for index in 0..lines.len() {
        if lines[index].trim_start().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            continue;
        }
        let Some((worker_column, files_column, column_count, body_start)) =
            worker_table_header(&lines, index)
        else {
            continue;
        };
        if let Some(claims) = table_claims(
            &lines[body_start..],
            worker_column,
            files_column,
            column_count,
        ) {
            return claims;
        }
    }

    Vec::new()
}

fn worker_table_header(lines: &[&str], index: usize) -> Option<(usize, usize, usize, usize)> {
    let header = markdown_row(lines[index])?;
    let worker_column = named_column(&header, "Worker")?;
    let files_column = named_column(&header, "Files owned")?;
    let body_start = lines
        .get(index + 1)
        .and_then(|line| markdown_row(line))
        .filter(|row| is_divider_row(row, header.len()))
        .map_or(index + 1, |_| index + 2);

    Some((worker_column, files_column, header.len(), body_start))
}

fn named_column(cells: &[&str], name: &str) -> Option<usize> {
    let mut indices = cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.eq_ignore_ascii_case(name))
        .map(|(index, _)| index);
    let index = indices.next()?;
    indices.next().is_none().then_some(index)
}

fn markdown_row(line: &str) -> Option<Vec<&str>> {
    let line = line.trim();
    let content = line.strip_prefix('|')?.strip_suffix('|')?;
    Some(content.split('|').map(str::trim).collect())
}

fn is_divider_row(cells: &[&str], column_count: usize) -> bool {
    cells.len() == column_count
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            dashes.len() >= 3 && dashes.chars().all(|character| character == '-')
        })
}

/// Per the brief's absent-or-malformed rule, one malformed row makes the
/// whole table claim nothing.
fn table_claims(
    lines: &[&str],
    worker_column: usize,
    files_column: usize,
    column_count: usize,
) -> Option<Vec<WorkerClaim>> {
    let mut claims = Vec::new();

    for (row_index, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with('|') {
            break;
        }
        let row = markdown_row(line)?;
        if row.len() != column_count || is_divider_row(&row, column_count) {
            return None;
        }
        let worker = row[worker_column];
        if worker.is_empty() {
            continue;
        }
        claims.extend(row[files_column].split([',', ';']).filter_map(|path| {
            normalize_path(path).map(|path| WorkerClaim {
                worker: worker.to_string(),
                path,
                row: row_index,
            })
        }));
    }

    Some(claims)
}

fn normalize_path(path: &str) -> Option<String> {
    let path = path.trim().trim_matches('`').trim();
    let mut normalized = String::new();

    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::RootDir => normalized.push('/'),
            Component::ParentDir => append_component(&mut normalized, ".."),
            Component::Normal(component) => {
                append_component(&mut normalized, &component.to_string_lossy());
            }
            Component::Prefix(prefix) => {
                append_component(&mut normalized, &prefix.as_os_str().to_string_lossy());
            }
        }
    }

    (!normalized.is_empty()).then_some(normalized)
}

fn append_component(path: &mut String, component: &str) {
    if !path.is_empty() && !path.ends_with('/') {
        path.push('/');
    }
    path.push_str(component);
}

fn duplicate_claim_warnings(stage_id: &str, claims: &[WorkerClaim]) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut workers_by_path: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();

    for claim in claims {
        let workers = workers_by_path.entry(&claim.path).or_default();
        for (prior_worker, _prior_row) in workers.iter().copied().filter(|(prior_worker, row)| {
            *row != claim.row && *prior_worker != claim.worker.as_str()
        }) {
            warnings.push(format!(
                "Stage '{}': worker '{}' and worker '{}' both claim write ownership of '{}'",
                stage_id, prior_worker, claim.worker, claim.path
            ));
        }
        if !workers.iter().any(|(_, row)| *row == claim.row) {
            workers.push((&claim.worker, claim.row));
        }
    }

    warnings
}

fn outside_files_warnings(
    stage_id: &str,
    declared_files: &[String],
    claims: &[WorkerClaim],
) -> Vec<String> {
    claims
        .iter()
        .filter(|claim| !matches_declared_files(&claim.path, declared_files))
        .map(|claim| {
            format!(
                "Stage '{}': worker '{}' claims write ownership of '{}', which is outside the stage's declared files patterns",
                stage_id, claim.worker, claim.path
            )
        })
        .collect()
}

fn matches_declared_files(path: &str, declared_files: &[String]) -> bool {
    declared_files
        .iter()
        .filter_map(|pattern| normalize_path(pattern))
        .any(|pattern| {
            path == pattern
                || glob::Pattern::new(&pattern).is_ok_and(|pattern| pattern.matches(path))
        })
}
