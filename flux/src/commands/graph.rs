//! Execution graph display and editing
//! Usage: flux graph [show|edit]

use anyhow::{bail, Result};

/// Show the execution graph
pub fn show() -> Result<()> {
    println!("Execution Graph:");
    println!("═════════════════════════════════════════════════════════");

    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'flux init' first.");
    }

    let graph_file = work_dir.join("execution-graph.toml");
    if !graph_file.exists() {
        println!("(no execution graph - run 'flux init <plan>' to create one)");
        return Ok(());
    }

    // Read and display graph structure
    let content = std::fs::read_to_string(&graph_file)?;
    println!("{content}");

    Ok(())
}

/// Edit the execution graph (open in editor)
pub fn edit() -> Result<()> {
    let work_dir = std::env::current_dir()?.join(".work");
    if !work_dir.exists() {
        bail!(".work/ directory not found. Run 'flux init' first.");
    }

    let graph_file = work_dir.join("execution-graph.toml");
    if !graph_file.exists() {
        bail!("No execution graph found. Run 'flux init <plan>' first.");
    }

    // Try to open in $EDITOR or vim
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
    println!("Opening {} in {editor}", graph_file.display());
    println!(
        "Note: This would execute: {editor} {}",
        graph_file.display()
    );

    Ok(())
}
