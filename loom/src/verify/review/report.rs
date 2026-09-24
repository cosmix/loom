use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: String,
    pub file: String,
    pub line: u32,
    pub claim: String,
    pub scenario: Option<String>,
    pub rule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub file: Option<String>,
    pub line: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ParsedReview {
    pub findings: Vec<Finding>,
    pub suggestions: Vec<Suggestion>,
    pub resolved: Vec<String>,
    pub unresolved: Vec<String>,
}

/// `text` on one line: control characters and whitespace runs become single
/// spaces, so reviewer-authored text can neither span journal lines nor steer
/// a terminal. Every renderer of a review value goes through this.
pub fn single_line(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Err(reason) = malformed: no block, or the last block is not the D12 JSON shape.
pub fn parse_review(final_text: &str) -> Result<ParsedReview, String> {
    let block = last_review_block(final_text).ok_or("no loom-review block")?;
    let value: Value = serde_json::from_str(&block).map_err(|err| err.to_string())?;
    let object = value
        .as_object()
        .ok_or("loom-review block must be a JSON object")?;
    let mut review = ParsedReview {
        resolved: parse_ids(object, "resolved")?,
        unresolved: parse_ids(object, "unresolved")?,
        ..ParsedReview::default()
    };

    for value in array_field(object, "findings")? {
        parse_finding(value, &mut review)?;
    }
    for value in array_field(object, "suggestions")? {
        review.suggestions.push(parse_suggestion(value)?);
    }
    Ok(review)
}

fn last_review_block(text: &str) -> Option<String> {
    let mut open: Option<(u8, usize, bool, String)> = None;
    let mut last = None;
    for line in text.lines() {
        if let Some((marker, width, is_review, body)) = open.as_mut() {
            if let Some((closing, length, info)) = fence(line) {
                if closing == *marker && length == *width && info.trim().is_empty() {
                    if *is_review {
                        last = Some(std::mem::take(body));
                    }
                    open = None;
                    continue;
                }
            }
            if *is_review {
                body.push_str(line);
                body.push('\n');
            }
        } else if let Some((marker, width, info)) = fence(line) {
            open = Some((marker, width, info.trim() == "loom-review", String::new()));
        }
    }
    last
}

fn fence(line: &str) -> Option<(u8, usize, &str)> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let line = &line[indent..];
    let marker = *line.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let width = line.bytes().take_while(|byte| *byte == marker).count();
    (width >= 3).then_some((marker, width, &line[width..]))
}

fn array_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a [Value], String> {
    match object.get(key) {
        None => Ok(&[]),
        Some(value) => value
            .as_array()
            .map(Vec::as_slice)
            .ok_or_else(|| format!("{key} must be an array")),
    }
}

fn parse_ids(object: &Map<String, Value>, key: &str) -> Result<Vec<String>, String> {
    array_field(object, key)?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{key} must contain only string ids"))
        })
        .collect()
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a string or null")),
    }
}

fn line_field(object: &Map<String, Value>) -> Result<Option<u32>, String> {
    match object.get("line") {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            if value.as_i64().is_some_and(|line| line < 0) {
                return Ok(None);
            }
            let line = value.as_u64().ok_or("line must be an integer")?;
            u32::try_from(line)
                .map(Some)
                .map_err(|_| "line exceeds u32 range".to_owned())
        }
    }
}

fn parse_finding(value: &Value, review: &mut ParsedReview) -> Result<(), String> {
    let object = value.as_object().ok_or("finding must be an object")?;
    let file = string_field(object, "file")?.unwrap_or_default().to_owned();
    let line = line_field(object)?;
    let claim = string_field(object, "claim")?
        .unwrap_or_default()
        .to_owned();
    let scenario = string_field(object, "scenario")?.map(str::to_owned);
    let rule = string_field(object, "rule")?.map(str::to_owned);
    let valid = !file.is_empty()
        && line.is_some_and(|line| line >= 1)
        && !claim.is_empty()
        && (scenario.as_deref().is_some_and(|s| !s.is_empty())
            || rule.as_deref().is_some_and(|s| !s.is_empty()));
    if valid {
        let severity = string_field(object, "severity")?
            .unwrap_or_default()
            .to_ascii_lowercase();
        let severity = match severity.as_str() {
            "critical" | "major" | "minor" => severity,
            _ => "unspecified".to_owned(),
        };
        review.findings.push(Finding {
            severity,
            file,
            line: line.unwrap_or_default(),
            claim,
            scenario,
            rule,
        });
    } else {
        review.suggestions.push(Suggestion {
            file: string_field(object, "file")?.map(str::to_owned),
            line,
            text: claim,
        });
    }
    Ok(())
}

fn parse_suggestion(value: &Value) -> Result<Suggestion, String> {
    let object = value.as_object().ok_or("suggestion must be an object")?;
    Ok(Suggestion {
        file: string_field(object, "file")?.map(str::to_owned),
        line: line_field(object)?,
        text: string_field(object, "text")?
            .ok_or("suggestion text is required")?
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_review, single_line, Finding, Suggestion};

    #[test]
    fn single_line_flattens_control_characters_and_whitespace() {
        assert_eq!(single_line(" a\n\tb\u{1b}[31m  c \r\n"), "a b [31m c");
    }

    #[test]
    fn parses_loom_review_block() {
        let text = "Review notes\n```json\n{\"findings\":[]}\n```\n\
                    ```loom-review\n{\"findings\":[{\"severity\":\"MAJOR\",\"file\":\"src/a.rs\",\
                    \"line\":42,\"claim\":\"breaks\",\"scenario\":\"input -> failure\",\"rule\":null}],\
                    \"suggestions\":[{\"file\":null,\"line\":null,\"text\":\"simplify\"}],\
                    \"resolved\":[\"F-1-2\"]}\n```";
        let review = parse_review(text).unwrap();
        assert_eq!(
            review.findings,
            vec![Finding {
                severity: "major".into(),
                file: "src/a.rs".into(),
                line: 42,
                claim: "breaks".into(),
                scenario: Some("input -> failure".into()),
                rule: None,
            }]
        );
        assert_eq!(
            review.suggestions,
            vec![Suggestion {
                file: None,
                line: None,
                text: "simplify".into(),
            }]
        );
        assert_eq!(review.resolved, vec!["F-1-2".to_owned()]);
        assert!(review.unresolved.is_empty());
    }

    #[test]
    fn finding_without_scenario_or_rule_becomes_suggestion() {
        let text = "```loom-review\n{\"findings\":[{\"severity\":\"major\",\"file\":\"a.rs\",\
                    \"line\":2,\"claim\":\"consider this\",\"scenario\":null,\"rule\":\"\"}]}\n```";
        let review = parse_review(text).unwrap();
        assert!(review.findings.is_empty());
        assert_eq!(review.suggestions[0].text, "consider this");
        assert_eq!(review.suggestions[0].file.as_deref(), Some("a.rs"));
        assert_eq!(review.suggestions[0].line, Some(2));
    }

    #[test]
    fn no_block_is_an_error() {
        assert_eq!(
            parse_review("ordinary prose"),
            Err("no loom-review block".into())
        );
    }

    #[test]
    fn last_loom_review_block_wins() {
        let text = "~~~loom-review\n{\"resolved\":[\"old\"]}\n~~~\n\
                    ````loom-review\n{\"resolved\":[\"new\"]}\n````";
        assert_eq!(parse_review(text).unwrap().resolved, vec!["new".to_owned()]);
    }

    #[test]
    fn unknown_severity_becomes_unspecified() {
        let text = "```loom-review\n{\"findings\":[{\"severity\":\"urgent\",\"file\":\"a.rs\",\
                    \"line\":1,\"claim\":\"fails\",\"rule\":\"D12\"}]}\n```";
        assert_eq!(
            parse_review(text).unwrap().findings[0].severity,
            "unspecified"
        );
    }
}
