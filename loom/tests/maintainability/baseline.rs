use std::collections::{BTreeMap, BTreeSet};

use super::scanner::{
    FunctionMeasurement, SourceMeasurement, FILE_LINE_LIMIT, FUNCTION_LINE_LIMIT,
};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub enum BaselineKey {
    File { path: String },
    Function { path: String, name: String },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Violation {
    pub key: BaselineKey,
    pub measured: usize,
    pub location: String,
}

pub fn parse(source: &str) -> Result<BTreeMap<BaselineKey, usize>, Vec<String>> {
    let mut entries = BTreeMap::new();
    let mut errors = Vec::new();
    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match parse_line(line, line_number) {
            Ok((key, maximum)) => {
                if entries.insert(key.clone(), maximum).is_some() {
                    errors.push(format!(
                        "line {line_number}: duplicate baseline entry `{}`",
                        render_key(&key)
                    ));
                }
            }
            Err(error) => errors.push(error),
        }
    }
    errors.is_empty().then_some(entries).ok_or(errors)
}

fn parse_line(line: &str, line_number: usize) -> Result<(BaselineKey, usize), String> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let (key, maximum_text, limit) = match fields.as_slice() {
        ["file", path, maximum] => (
            BaselineKey::File {
                path: validate_path(path, line_number)?,
            },
            *maximum,
            FILE_LINE_LIMIT,
        ),
        ["function", path, name, maximum] => (
            BaselineKey::Function {
                path: validate_path(path, line_number)?,
                name: validate_name(name, line_number)?,
            },
            *maximum,
            FUNCTION_LINE_LIMIT,
        ),
        _ => {
            return Err(format!(
                "line {line_number}: expected `file <path> <lines>` or `function <path> <name> <lines>`"
            ));
        }
    };
    let maximum = maximum_text.parse::<usize>().map_err(|_| {
        format!("line {line_number}: `{maximum_text}` is not a positive line count")
    })?;
    if maximum <= limit {
        return Err(format!(
            "line {line_number}: `{}` records {maximum} lines, which does not exceed the {limit}-line limit; remove it",
            render_key(&key)
        ));
    }
    Ok((key, maximum))
}

fn validate_path(path: &str, line_number: usize) -> Result<String, String> {
    let valid_root = path.starts_with("src/") || path.starts_with("tests/");
    let valid = valid_root
        && path.ends_with(".rs")
        && !path.contains("//")
        && !path.split('/').any(|part| matches!(part, "" | "." | ".."));
    if !valid {
        return Err(format!(
            "line {line_number}: baseline path must be a normalized src/**/*.rs or tests/**/*.rs path: `{path}`"
        ));
    }
    Ok(path.to_string())
}

fn validate_name(name: &str, line_number: usize) -> Result<String, String> {
    let valid = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'#'));
    if !valid {
        return Err(format!(
            "line {line_number}: invalid function name `{name}` in baseline"
        ));
    }
    Ok(name.to_string())
}

pub fn current_violations(measurements: &[SourceMeasurement]) -> Vec<Violation> {
    let mut violations = Vec::new();
    for source in measurements {
        if source.lines > FILE_LINE_LIMIT {
            violations.push(Violation {
                key: BaselineKey::File {
                    path: source.path.clone(),
                },
                measured: source.lines,
                location: source.path.clone(),
            });
        }
        violations.extend(function_violations(source));
    }
    violations.sort_by(|left, right| left.key.cmp(&right.key));
    violations
}

fn function_violations(source: &SourceMeasurement) -> Vec<Violation> {
    let mut totals = BTreeMap::<&str, usize>::new();
    for function in &source.functions {
        *totals.entry(function.name.as_str()).or_default() += 1;
    }

    let mut seen = BTreeMap::<&str, usize>::new();
    let mut violations = Vec::new();
    for function in &source.functions {
        let occurrence = seen.entry(function.name.as_str()).or_default();
        *occurrence += 1;
        if function.lines() <= FUNCTION_LINE_LIMIT {
            continue;
        }
        let name = occurrence_qualified_name(function, *occurrence, &totals);
        violations.push(Violation {
            key: BaselineKey::Function {
                path: source.path.clone(),
                name,
            },
            measured: function.lines(),
            location: format!("{}:{}", source.path, function.start_line),
        });
    }
    violations
}

fn occurrence_qualified_name(
    function: &FunctionMeasurement,
    occurrence: usize,
    totals: &BTreeMap<&str, usize>,
) -> String {
    if totals[function.name.as_str()] == 1 {
        function.name.clone()
    } else {
        format!("{}#{occurrence}", function.name)
    }
}

pub fn validate(
    baseline: &BTreeMap<BaselineKey, usize>,
    violations: &[Violation],
) -> Result<(), Vec<String>> {
    let current = violations
        .iter()
        .map(|violation| (violation.key.clone(), violation))
        .collect::<BTreeMap<_, _>>();
    let mut errors = validate_recorded_entries(baseline, &current);
    errors.extend(find_unrecorded_violations(baseline, violations));
    errors.is_empty().then_some(()).ok_or(errors)
}

fn validate_recorded_entries(
    baseline: &BTreeMap<BaselineKey, usize>,
    current: &BTreeMap<BaselineKey, &Violation>,
) -> Vec<String> {
    let mut errors = Vec::new();
    for (key, recorded) in baseline {
        match current.get(key) {
            None => errors.push(format!(
                "stale entry `{}` no longer violates its limit; remove it",
                render_entry(key, *recorded)
            )),
            Some(violation) if violation.measured < *recorded => errors.push(format!(
                "{} shrank from {recorded} to {} lines; lower the entry to `{}`",
                violation.location,
                violation.measured,
                render_entry(key, violation.measured)
            )),
            Some(violation) if violation.measured > *recorded => errors.push(format!(
                "{} grew from {recorded} to {} lines; refactor it back to {recorded} or less",
                violation.location, violation.measured
            )),
            Some(_) => {}
        }
    }
    errors
}

fn find_unrecorded_violations(
    baseline: &BTreeMap<BaselineKey, usize>,
    violations: &[Violation],
) -> Vec<String> {
    let known = baseline.keys().collect::<BTreeSet<_>>();
    violations
        .iter()
        .filter(|violation| !known.contains(&violation.key))
        .map(|violation| {
            format!(
                "new violation at {} ({} lines); refactor it or add `{}`",
                violation.location,
                violation.measured,
                render_entry(&violation.key, violation.measured)
            )
        })
        .collect()
}

fn render_key(key: &BaselineKey) -> String {
    match key {
        BaselineKey::File { path } => format!("file {path}"),
        BaselineKey::Function { path, name } => format!("function {path} {name}"),
    }
}

pub fn render_entry(key: &BaselineKey, maximum: usize) -> String {
    format!("{} {maximum}", render_key(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, lines: usize) -> SourceMeasurement {
        SourceMeasurement {
            path: path.to_string(),
            lines,
            functions: Vec::new(),
        }
    }

    #[test]
    fn new_oversized_file_requires_an_explicit_entry() {
        let violations = current_violations(&[file("src/new.rs", 401)]);
        let errors = validate(&BTreeMap::new(), &violations).unwrap_err();

        assert!(errors[0].contains("add `file src/new.rs 401`"));
    }

    #[test]
    fn new_oversized_function_requires_an_explicit_entry() {
        let mut source = file("src/new.rs", 100);
        source.functions.push(FunctionMeasurement {
            name: "oversized".to_string(),
            start_line: 10,
            end_line: 60,
        });

        let violations = current_violations(&[source]);
        let errors = validate(&BTreeMap::new(), &violations).unwrap_err();

        assert!(errors[0].contains("add `function src/new.rs oversized 51`"));
    }

    #[test]
    fn duplicate_function_names_have_independent_entries() {
        let mut source = file("src/impls.rs", 100);
        source.functions = vec![function("shared", 1, 51), function("shared", 60, 115)];

        let violations = current_violations(&[source]);
        let baseline = parse("function src/impls.rs shared#1 51\n").unwrap();
        let errors = validate(&baseline, &violations).unwrap_err();

        assert_eq!(violations.len(), 2);
        assert!(errors
            .iter()
            .any(|error| error.contains("add `function src/impls.rs shared#2 56`")));
    }

    #[test]
    fn stale_and_shrunken_entries_require_removal_or_lowering() {
        let stale = parse("file src/clean.rs 401\n").unwrap();
        let stale_errors = validate(&stale, &[]).unwrap_err();
        assert!(stale_errors[0].contains("remove it"));

        let shrunken = parse("file src/large.rs 410\n").unwrap();
        let current = current_violations(&[file("src/large.rs", 405)]);
        let shrink_errors = validate(&shrunken, &current).unwrap_err();
        assert!(shrink_errors[0].contains("lower the entry to `file src/large.rs 405`"));
    }

    #[test]
    fn duplicate_and_nonviolating_entries_are_rejected() {
        let errors =
            parse("file src/large.rs 401\nfile src/large.rs 401\nfunction src/lib.rs short 50\n")
                .unwrap_err();

        assert!(errors.iter().any(|error| error.contains("duplicate")));
        assert!(errors.iter().any(|error| error.contains("does not exceed")));
    }

    fn function(name: &str, start_line: usize, end_line: usize) -> FunctionMeasurement {
        FunctionMeasurement {
            name: name.to_string(),
            start_line,
            end_line,
        }
    }
}
