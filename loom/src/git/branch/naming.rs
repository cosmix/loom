//! Branch naming conventions for loom stages

/// Generate loom branch name from stage ID
pub fn branch_name_for_stage(stage_id: &str) -> String {
    format!("loom/{stage_id}")
}

/// Get the stage ID from a loom branch name
pub fn stage_id_from_branch(branch_name: &str) -> Option<String> {
    branch_name.strip_prefix("loom/").map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_id_from_branch() {
        assert_eq!(
            stage_id_from_branch("loom/stage-1"),
            Some("stage-1".to_string())
        );
        assert_eq!(stage_id_from_branch("main"), None);
    }

    #[test]
    fn test_branch_name_for_stage() {
        assert_eq!(branch_name_for_stage("stage-1"), "loom/stage-1");
        assert_eq!(branch_name_for_stage("my-feature"), "loom/my-feature");
    }
}
