//! Branch information types and parsing

use anyhow::Result;

/// Branch information
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    pub commit_hash: String,
    pub commit_message: String,
}

/// Parse git branch -v output
pub(super) fn parse_branch_list(output: &str) -> Result<Vec<BranchInfo>> {
    let mut branches = Vec::new();

    for line in output.lines() {
        if line.is_empty() {
            continue;
        }

        let is_current = line.starts_with('*');
        let line = line.trim_start_matches('*').trim();

        // Parse: branch_name commit_hash commit_message
        let parts: Vec<&str> = line.splitn(3, ' ').collect();
        if parts.len() >= 2 {
            let name = parts[0].trim().to_string();
            let commit_hash = parts[1].trim().to_string();
            let commit_message = if parts.len() > 2 {
                parts[2].trim().to_string()
            } else {
                String::new()
            };

            branches.push(BranchInfo {
                name,
                is_current,
                commit_hash,
                commit_message,
            });
        }
    }

    Ok(branches)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_branch_list() {
        let output = r#"* main       abc1234 Initial commit
  loom/stage-1 def5678 Add feature
  feature    789abcd Work in progress
"#;

        let branches = parse_branch_list(output).unwrap();
        assert_eq!(branches.len(), 3);

        assert_eq!(branches[0].name, "main");
        assert!(branches[0].is_current);

        assert_eq!(branches[1].name, "loom/stage-1");
        assert!(!branches[1].is_current);
    }
}
