//! Execution graph display and editing
//! Usage: loom graph [show|edit]

use anyhow::{bail, Result};
use colored::{ColoredString, Colorize};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::list_all_stages;

/// Status indicator with color for display
fn status_indicator(status: &StageStatus) -> ColoredString {
    match status {
        StageStatus::Completed => "✓".green().bold(),
        StageStatus::Executing => "●".blue().bold(),
        StageStatus::Queued => "▶".cyan().bold(),
        StageStatus::WaitingForDeps => "○".white().dimmed(),
        StageStatus::WaitingForInput => "?".magenta().bold(),
        StageStatus::Blocked => "✗".red().bold(),
        StageStatus::NeedsHandoff => "⟳".yellow().bold(),
    }
}

/// Compute the topological level for each stage.
/// Level = max(levels of all dependencies) + 1, with roots at level 0.
fn compute_stage_levels(stages: &[Stage]) -> HashMap<String, usize> {
    let stage_map: HashMap<&str, &Stage> = stages.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut levels: HashMap<String, usize> = HashMap::new();

    fn get_level(
        stage_id: &str,
        stage_map: &HashMap<&str, &Stage>,
        levels: &mut HashMap<String, usize>,
        visiting: &mut HashSet<String>,
    ) -> usize {
        if let Some(&level) = levels.get(stage_id) {
            return level;
        }

        // Cycle detection - treat as level 0 to avoid infinite recursion
        if visiting.contains(stage_id) {
            return 0;
        }
        visiting.insert(stage_id.to_string());

        let stage = match stage_map.get(stage_id) {
            Some(s) => s,
            None => return 0,
        };

        let level = if stage.dependencies.is_empty() {
            0
        } else {
            stage
                .dependencies
                .iter()
                .map(|dep| get_level(dep, stage_map, levels, visiting))
                .max()
                .unwrap_or(0)
                + 1
        };

        visiting.remove(stage_id);
        levels.insert(stage_id.to_string(), level);
        level
    }

    for stage in stages {
        let mut visiting = HashSet::new();
        get_level(&stage.id, &stage_map, &mut levels, &mut visiting);
    }

    levels
}

/// Sort stages by status priority (executing first, then ready, then others)
fn status_priority(status: &StageStatus) -> u8 {
    match status {
        StageStatus::Executing => 0,
        StageStatus::Queued => 1,
        StageStatus::WaitingForInput => 2,
        StageStatus::NeedsHandoff => 3,
        StageStatus::WaitingForDeps => 4,
        StageStatus::Blocked => 5,
        StageStatus::Completed => 6,
    }
}

/// Format dependencies with their status indicators
fn format_dependencies(stage: &Stage, stage_map: &HashMap<&str, &Stage>) -> String {
    if stage.dependencies.is_empty() {
        return String::new();
    }

    let dep_strs: Vec<String> = stage
        .dependencies
        .iter()
        .map(|dep_id| {
            if let Some(dep_stage) = stage_map.get(dep_id.as_str()) {
                let ind = status_indicator(&dep_stage.status);
                format!("{ind}{dep_id}")
            } else {
                format!("?{dep_id}")
            }
        })
        .collect();

    format!(" ← {}", dep_strs.join(", "))
}

/// Build a visual representation of the dependency graph using layered levels
pub fn build_graph_display(stages: &[Stage]) -> Result<String> {
    if stages.is_empty() {
        return Ok("(no stages found - run 'loom init <plan>' to create stages)".to_string());
    }

    let stage_map: HashMap<&str, &Stage> = stages.iter().map(|s| (s.id.as_str(), s)).collect();
    let levels = compute_stage_levels(stages);

    // Group stages by level (BTreeMap for sorted keys)
    let mut by_level: BTreeMap<usize, Vec<&Stage>> = BTreeMap::new();
    for stage in stages {
        let level = levels.get(&stage.id).copied().unwrap_or(0);
        by_level.entry(level).or_default().push(stage);
    }

    // Sort stages within each level by status priority, then by id
    for stages_in_level in by_level.values_mut() {
        stages_in_level.sort_by(|a, b| {
            status_priority(&a.status)
                .cmp(&status_priority(&b.status))
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    let mut output = String::new();

    for (level, stages_in_level) in &by_level {
        // Level header
        let header = if *level == 0 {
            "Level 0 (no dependencies):".to_string()
        } else {
            format!("Level {level}:")
        };
        output.push_str(&header);
        output.push('\n');

        // Render each stage in this level
        for stage in stages_in_level {
            let indicator = status_indicator(&stage.status);
            let deps = format_dependencies(stage, &stage_map);
            output.push_str(&format!(
                "  {indicator} {} ({}){deps}\n",
                stage.name, stage.id
            ));
        }

        output.push('\n');
    }

    Ok(output)
}

/// Show the execution graph
pub fn show() -> Result<()> {
    println!();
    println!("Execution Graph:");
    println!("================");
    println!();

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    let stages = list_all_stages(&work_dir)?;
    let graph_display = build_graph_display(&stages)?;
    println!("{graph_display}");

    // Print legend with colored symbols
    println!();
    print!("Legend: ");
    print!("{} ", "✓".green().bold());
    print!("completed  ");
    print!("{} ", "●".blue().bold());
    print!("executing  ");
    print!("{} ", "▶".cyan().bold());
    print!("ready  ");
    print!("{} ", "○".white().dimmed());
    print!("pending  ");
    print!("{} ", "?".magenta().bold());
    print!("waiting  ");
    print!("{} ", "✗".red().bold());
    print!("blocked  ");
    print!("{} ", "⟳".yellow().bold());
    println!("handoff");
    println!();

    Ok(())
}

/// Edit the execution graph by opening the stages directory
///
/// The execution graph is dynamically built from stage files in `.work/stages/`.
/// This command opens the stages directory in the configured editor, allowing
/// direct modification of stage files.
pub fn edit() -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'loom init' first.");
    }

    let stages_dir = work_dir.join("stages");
    if !stages_dir.exists() {
        bail!("No stages directory found. Run 'loom init <plan>' first.");
    }

    // Check if there are any stage files
    let stage_files: Vec<_> = std::fs::read_dir(&stages_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    if stage_files.is_empty() {
        bail!("No stage files found. Run 'loom init <plan>' first.");
    }

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    println!(
        "The execution graph is built from stage files in: {}",
        stages_dir.display()
    );
    println!();
    println!("Stage files:");
    for entry in &stage_files {
        println!("  - {}", entry.path().display());
    }
    println!();
    println!("To edit a stage, run:");
    println!("  {editor} {}/[stage-id].md", stages_dir.display());
    println!();
    println!("Each stage file contains YAML frontmatter with:");
    println!("  - id, name, status, dependencies, parallel_group, acceptance, files");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;

    fn create_test_stage(id: &str, name: &str, status: StageStatus, deps: Vec<&str>) -> Stage {
        let mut stage = Stage::new(name.to_string(), Some(format!("Test stage: {name}")));
        stage.id = id.to_string();
        stage.status = status;
        stage.dependencies = deps.into_iter().map(String::from).collect();
        stage
    }

    #[test]
    fn test_status_indicator() {
        // Test that indicators contain the expected Unicode symbols
        // (colored strings include ANSI codes, so we check the base character)
        assert!(status_indicator(&StageStatus::Completed)
            .to_string()
            .contains('✓'));
        assert!(status_indicator(&StageStatus::Executing)
            .to_string()
            .contains('●'));
        assert!(status_indicator(&StageStatus::Queued)
            .to_string()
            .contains('▶'));
        assert!(status_indicator(&StageStatus::WaitingForDeps)
            .to_string()
            .contains('○'));
        assert!(status_indicator(&StageStatus::WaitingForInput)
            .to_string()
            .contains('?'));
        assert!(status_indicator(&StageStatus::Blocked)
            .to_string()
            .contains('✗'));
        assert!(status_indicator(&StageStatus::NeedsHandoff)
            .to_string()
            .contains('⟳'));
    }

    #[test]
    fn test_build_graph_display_empty() {
        let stages: Vec<Stage> = vec![];
        let output = build_graph_display(&stages).unwrap();
        assert!(output.contains("no stages found"));
    }

    #[test]
    fn test_build_graph_display_single_stage() {
        let stages = vec![create_test_stage(
            "stage-1",
            "First Stage",
            StageStatus::Queued,
            vec![],
        )];

        let output = build_graph_display(&stages).unwrap();
        assert!(output.contains('▶')); // Ready indicator
        assert!(output.contains("First Stage"));
        assert!(output.contains("stage-1"));
        assert!(output.contains("Level 0"), "Single stage should be at level 0");
    }

    #[test]
    fn test_build_graph_display_linear_chain() {
        let stages = vec![
            create_test_stage("stage-1", "First", StageStatus::Completed, vec![]),
            create_test_stage("stage-2", "Second", StageStatus::Executing, vec!["stage-1"]),
            create_test_stage(
                "stage-3",
                "Third",
                StageStatus::WaitingForDeps,
                vec!["stage-2"],
            ),
        ];

        let output = build_graph_display(&stages).unwrap();

        // Check that all stages appear
        assert!(output.contains("First"));
        assert!(output.contains("Second"));
        assert!(output.contains("Third"));

        // Check status indicators
        assert!(output.contains('✓')); // Completed
        assert!(output.contains('●')); // Executing
        assert!(output.contains('○')); // Pending

        // Check level structure for linear chain
        assert!(output.contains("Level 0"), "Should have level 0");
        assert!(output.contains("Level 1:"), "Should have level 1");
        assert!(output.contains("Level 2:"), "Should have level 2");

        // Second should show dependency on First
        assert!(output.contains("← "), "Should show dependency arrows");
    }

    #[test]
    fn test_build_graph_display_diamond_pattern() {
        // Diamond: A -> B, A -> C, B -> D, C -> D
        let stages = vec![
            create_test_stage("a", "Stage A", StageStatus::Completed, vec![]),
            create_test_stage("b", "Stage B", StageStatus::Completed, vec!["a"]),
            create_test_stage("c", "Stage C", StageStatus::Completed, vec!["a"]),
            create_test_stage("d", "Stage D", StageStatus::Queued, vec!["b", "c"]),
        ];

        let output = build_graph_display(&stages).unwrap();

        // All stages should be present
        assert!(output.contains("Stage A"));
        assert!(output.contains("Stage B"));
        assert!(output.contains("Stage C"));
        assert!(output.contains("Stage D"));

        // Check level structure: A at level 0, B and C at level 1, D at level 2
        assert!(output.contains("Level 0"), "Should have level 0 header");
        assert!(output.contains("Level 1:"), "Should have level 1 header");
        assert!(output.contains("Level 2:"), "Should have level 2 header");

        // D should show ALL its dependencies (both b and c)
        assert!(
            output.contains("← ") && output.contains("b") && output.contains("c"),
            "Diamond node D should show all dependencies"
        );
    }

    #[test]
    fn test_build_graph_display_shows_all_deps() {
        // Simulate the user's scenario: integration-tests depends on 3 stages,
        // one of which is still executing
        let stages = vec![
            create_test_stage("state-machine", "State Machine", StageStatus::Completed, vec![]),
            create_test_stage(
                "merge-completed",
                "Merge Completed",
                StageStatus::Completed,
                vec!["state-machine"],
            ),
            create_test_stage(
                "complete-refactor",
                "Complete Refactor",
                StageStatus::Executing,
                vec!["state-machine"],
            ),
            create_test_stage(
                "criteria-validation",
                "Criteria Validation",
                StageStatus::Completed,
                vec![],
            ),
            create_test_stage(
                "context-vars",
                "Context Variables",
                StageStatus::Completed,
                vec!["criteria-validation"],
            ),
            create_test_stage(
                "integration-tests",
                "Integration Tests",
                StageStatus::WaitingForDeps,
                vec!["complete-refactor", "merge-completed", "context-vars"],
            ),
        ];

        let output = build_graph_display(&stages).unwrap();

        // integration-tests should be present at level 2
        assert!(
            output.contains("integration-tests"),
            "Integration tests stage should be present"
        );

        // Should show ALL dependencies with "←" (not "← also:")
        assert!(
            output.contains("← "),
            "Multi-dep stage should show dependencies"
        );

        // The output should show all dependencies including the blocking one
        // (complete-refactor with ● indicator)
        assert!(
            output.contains("complete-refactor") && output.contains("merge-completed") && output.contains("context-vars"),
            "Should show all dependencies for integration-tests"
        );

        // Verify level structure for this complex graph
        assert!(output.contains("Level 0"), "Should have level 0");
        assert!(output.contains("Level 1:"), "Should have level 1");
        assert!(output.contains("Level 2:"), "Should have level 2");
    }

    #[test]
    fn test_build_graph_display_multiple_roots() {
        let stages = vec![
            create_test_stage("root-1", "Root One", StageStatus::Queued, vec![]),
            create_test_stage("root-2", "Root Two", StageStatus::Executing, vec![]),
            create_test_stage(
                "child",
                "Child",
                StageStatus::WaitingForDeps,
                vec!["root-1", "root-2"],
            ),
        ];

        let output = build_graph_display(&stages).unwrap();

        // All stages present
        assert!(output.contains("Root One"));
        assert!(output.contains("Root Two"));
        assert!(output.contains("Child"));

        // Both roots at level 0, child at level 1
        assert!(output.contains("Level 0"), "Should have level 0");
        assert!(output.contains("Level 1:"), "Should have level 1");

        // Child should show both dependencies
        assert!(
            output.contains("root-1") && output.contains("root-2"),
            "Child should show both parent dependencies"
        );
    }

    #[test]
    fn test_build_graph_display_all_statuses() {
        let stages = vec![
            create_test_stage("s1", "Completed Stage", StageStatus::Completed, vec![]),
            create_test_stage("s2", "Executing Stage", StageStatus::Executing, vec![]),
            create_test_stage("s3", "Ready Stage", StageStatus::Queued, vec![]),
            create_test_stage("s4", "Pending Stage", StageStatus::WaitingForDeps, vec![]),
            create_test_stage(
                "s5",
                "WaitingForInput Stage",
                StageStatus::WaitingForInput,
                vec![],
            ),
            create_test_stage("s6", "Blocked Stage", StageStatus::Blocked, vec![]),
            create_test_stage(
                "s7",
                "NeedsHandoff Stage",
                StageStatus::NeedsHandoff,
                vec![],
            ),
        ];

        let output = build_graph_display(&stages).unwrap();

        // Check all Unicode status indicators are present
        assert!(output.contains('✓')); // Completed
        assert!(output.contains('●')); // Executing
        assert!(output.contains('▶')); // Ready
        assert!(output.contains('○')); // Pending
        assert!(output.contains('?')); // WaitingForInput
        assert!(output.contains('✗')); // Blocked
        assert!(output.contains('⟳')); // NeedsHandoff
    }

    #[test]
    fn test_build_graph_display_status_sorting() {
        // Stages with different statuses at same level - should be sorted by priority
        let stages = vec![
            create_test_stage("a", "Completed A", StageStatus::Completed, vec![]),
            create_test_stage("b", "Executing B", StageStatus::Executing, vec![]),
            create_test_stage("c", "Ready C", StageStatus::Queued, vec![]),
        ];

        let output = build_graph_display(&stages).unwrap();

        // Executing should appear before Ready, which should appear before Completed
        let pos_exec = output.find("Executing B").unwrap();
        let pos_ready = output.find("Ready C").unwrap();
        let pos_completed = output.find("Completed A").unwrap();

        assert!(
            pos_exec < pos_ready && pos_ready < pos_completed,
            "Stages should be sorted by status priority: executing < ready < completed"
        );
    }

    #[test]
    fn test_compute_stage_levels() {
        let stages = vec![
            create_test_stage("a", "A", StageStatus::Completed, vec![]),
            create_test_stage("b", "B", StageStatus::Completed, vec!["a"]),
            create_test_stage("c", "C", StageStatus::Completed, vec!["a"]),
            create_test_stage("d", "D", StageStatus::Completed, vec!["b", "c"]),
        ];

        let levels = compute_stage_levels(&stages);

        assert_eq!(levels.get("a"), Some(&0));
        assert_eq!(levels.get("b"), Some(&1));
        assert_eq!(levels.get("c"), Some(&1));
        assert_eq!(levels.get("d"), Some(&2));
    }
}
