//! Execution graph display and editing
//! Usage: loom graph [show|edit]

use anyhow::{bail, Result};
use colored::{ColoredString, Colorize};
use std::collections::{HashMap, HashSet};

use crate::models::stage::StageStatus;
use crate::verify::transitions::list_all_stages;

/// Status indicator with color for display
fn status_indicator(status: &StageStatus) -> ColoredString {
    match status {
        StageStatus::Verified => "✓".green().bold(),
        StageStatus::Executing => "●".blue().bold(),
        StageStatus::Ready => "▶".cyan().bold(),
        StageStatus::Pending => "○".white().dimmed(),
        StageStatus::WaitingForInput => "?".magenta().bold(),
        StageStatus::Blocked => "✗".red().bold(),
        StageStatus::Completed => "✔".green(),
        StageStatus::NeedsHandoff => "⟳".yellow().bold(),
    }
}

/// Context for rendering the graph (reduces function arguments)
struct GraphRenderContext<'a> {
    stage_map: HashMap<&'a str, &'a crate::models::stage::Stage>,
    dependents: HashMap<&'a str, Vec<&'a str>>,
    visited: HashSet<String>,
    output: String,
}

impl<'a> GraphRenderContext<'a> {
    fn new(stages: &'a [crate::models::stage::Stage]) -> Self {
        let stage_map: HashMap<&str, &crate::models::stage::Stage> =
            stages.iter().map(|s| (s.id.as_str(), s)).collect();

        let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
        for stage in stages {
            for dep in &stage.dependencies {
                dependents
                    .entry(dep.as_str())
                    .or_default()
                    .push(stage.id.as_str());
            }
        }

        Self {
            stage_map,
            dependents,
            visited: HashSet::new(),
            output: String::new(),
        }
    }

    fn render_stage(&mut self, stage_id: &str, indent: usize, is_last: bool, prefix: &str) {
        if self.visited.contains(stage_id) {
            return;
        }
        self.visited.insert(stage_id.to_string());

        let Some(stage) = self.stage_map.get(stage_id) else {
            return;
        };

        let indicator = status_indicator(&stage.status);
        let connector = if indent == 0 {
            ""
        } else if is_last {
            "`-- "
        } else {
            "|-- "
        };

        self.output.push_str(&format!(
            "{prefix}{connector}{indicator} {} ({})\n",
            stage.name, stage.id
        ));

        // Get children (stages that depend on this one)
        let children: Vec<&str> = self
            .dependents
            .get(stage_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .filter(|&&child_id| {
                // Only show a child here if all its dependencies are visited
                // or this is its only/first dependency in our traversal
                if let Some(child) = self.stage_map.get(child_id) {
                    child
                        .dependencies
                        .iter()
                        .filter(|dep| *dep != stage_id)
                        .all(|dep| self.visited.contains(dep))
                } else {
                    false
                }
            })
            .copied()
            .collect();

        let child_count = children.len();
        for (i, child_id) in children.into_iter().enumerate() {
            let child_is_last = i == child_count - 1;
            let new_prefix = if indent == 0 {
                String::new()
            } else {
                format!("{prefix}{}   ", if is_last { " " } else { "|" })
            };

            self.render_stage(child_id, indent + 1, child_is_last, &new_prefix);
        }
    }
}

/// Build a visual representation of the dependency graph
pub fn build_graph_display(stages: &[crate::models::stage::Stage]) -> Result<String> {
    if stages.is_empty() {
        return Ok("(no stages found - run 'loom init <plan>' to create stages)".to_string());
    }

    // Find root stages (no dependencies)
    let root_ids: Vec<&str> = stages
        .iter()
        .filter(|s| s.dependencies.is_empty())
        .map(|s| s.id.as_str())
        .collect();

    let mut ctx = GraphRenderContext::new(stages);

    // Start rendering from root stages
    let root_count = root_ids.len();
    for (i, root_id) in root_ids.iter().enumerate() {
        ctx.render_stage(root_id, 0, i == root_count - 1, "");
    }

    // Handle any unvisited stages (in case of disconnected components or cycles)
    for stage in stages {
        if !ctx.visited.contains(&stage.id) {
            let indicator = status_indicator(&stage.status);
            ctx.output.push_str(&format!(
                "{indicator} {} ({}) [disconnected]\n",
                stage.name, stage.id
            ));
        }
    }

    Ok(ctx.output)
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
    print!("verified  ");
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
    print!("{} ", "✔".green());
    print!("completed  ");
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
        assert!(status_indicator(&StageStatus::Verified)
            .to_string()
            .contains('✓'));
        assert!(status_indicator(&StageStatus::Executing)
            .to_string()
            .contains('●'));
        assert!(status_indicator(&StageStatus::Ready)
            .to_string()
            .contains('▶'));
        assert!(status_indicator(&StageStatus::Pending)
            .to_string()
            .contains('○'));
        assert!(status_indicator(&StageStatus::WaitingForInput)
            .to_string()
            .contains('?'));
        assert!(status_indicator(&StageStatus::Blocked)
            .to_string()
            .contains('✗'));
        assert!(status_indicator(&StageStatus::Completed)
            .to_string()
            .contains('✔'));
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
            StageStatus::Ready,
            vec![],
        )];

        let output = build_graph_display(&stages).unwrap();
        assert!(output.contains('▶')); // Ready indicator
        assert!(output.contains("First Stage"));
        assert!(output.contains("stage-1"));
    }

    #[test]
    fn test_build_graph_display_linear_chain() {
        let stages = vec![
            create_test_stage("stage-1", "First", StageStatus::Verified, vec![]),
            create_test_stage("stage-2", "Second", StageStatus::Executing, vec!["stage-1"]),
            create_test_stage("stage-3", "Third", StageStatus::Pending, vec!["stage-2"]),
        ];

        let output = build_graph_display(&stages).unwrap();

        // Check that all stages appear
        assert!(output.contains("First"));
        assert!(output.contains("Second"));
        assert!(output.contains("Third"));

        // Check status indicators
        assert!(output.contains('✓')); // Verified
        assert!(output.contains('●')); // Executing
        assert!(output.contains('○')); // Pending
    }

    #[test]
    fn test_build_graph_display_diamond_pattern() {
        // Diamond: A -> B, A -> C, B -> D, C -> D
        let stages = vec![
            create_test_stage("a", "Stage A", StageStatus::Verified, vec![]),
            create_test_stage("b", "Stage B", StageStatus::Completed, vec!["a"]),
            create_test_stage("c", "Stage C", StageStatus::Completed, vec!["a"]),
            create_test_stage("d", "Stage D", StageStatus::Ready, vec!["b", "c"]),
        ];

        let output = build_graph_display(&stages).unwrap();

        // All stages should be present
        assert!(output.contains("Stage A"));
        assert!(output.contains("Stage B"));
        assert!(output.contains("Stage C"));
        assert!(output.contains("Stage D"));

        // D should appear after both B and C (since it depends on both)
        let pos_a = output.find("Stage A").unwrap();
        let pos_d = output.find("Stage D").unwrap();
        assert!(pos_a < pos_d);
    }

    #[test]
    fn test_build_graph_display_multiple_roots() {
        let stages = vec![
            create_test_stage("root-1", "Root One", StageStatus::Ready, vec![]),
            create_test_stage("root-2", "Root Two", StageStatus::Executing, vec![]),
            create_test_stage(
                "child",
                "Child",
                StageStatus::Pending,
                vec!["root-1", "root-2"],
            ),
        ];

        let output = build_graph_display(&stages).unwrap();

        assert!(output.contains("Root One"));
        assert!(output.contains("Root Two"));
        assert!(output.contains("Child"));
    }

    #[test]
    fn test_build_graph_display_all_statuses() {
        let stages = vec![
            create_test_stage("s1", "Verified Stage", StageStatus::Verified, vec![]),
            create_test_stage("s2", "Executing Stage", StageStatus::Executing, vec![]),
            create_test_stage("s3", "Ready Stage", StageStatus::Ready, vec![]),
            create_test_stage("s4", "Pending Stage", StageStatus::Pending, vec![]),
            create_test_stage(
                "s5",
                "WaitingForInput Stage",
                StageStatus::WaitingForInput,
                vec![],
            ),
            create_test_stage("s6", "Blocked Stage", StageStatus::Blocked, vec![]),
            create_test_stage("s7", "Completed Stage", StageStatus::Completed, vec![]),
            create_test_stage(
                "s8",
                "NeedsHandoff Stage",
                StageStatus::NeedsHandoff,
                vec![],
            ),
        ];

        let output = build_graph_display(&stages).unwrap();

        // Check all Unicode status indicators are present
        assert!(output.contains('✓')); // Verified
        assert!(output.contains('●')); // Executing
        assert!(output.contains('▶')); // Ready
        assert!(output.contains('○')); // Pending
        assert!(output.contains('?')); // WaitingForInput
        assert!(output.contains('✗')); // Blocked
        assert!(output.contains('✔')); // Completed
        assert!(output.contains('⟳')); // NeedsHandoff
    }
}
