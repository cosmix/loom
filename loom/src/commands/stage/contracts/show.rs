//! `loom stage contracts show`: print a stage's freeze record and where its
//! frozen copies are kept.

use anyhow::Result;

use crate::verify::contracts::store::{frozen_file_path, load_freeze};

/// `loom stage contracts show <stage-id>`.
pub fn show(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let Some(record) = load_freeze(&work_dir, &stage_id)? else {
        println!("Stage '{stage_id}' has no frozen contracts.");
        return Ok(());
    };

    println!(
        "Contracts of stage '{stage_id}', frozen at {} by session {} against base {}",
        record.frozen_at, record.session_id, record.base
    );
    println!();
    println!("Contracts:");
    for contract in &record.contracts {
        let adapter = contract.adapter.as_deref().unwrap_or("unsupported");
        let exit = contract
            .exit_code
            .map_or_else(|| "none".to_string(), |code| code.to_string());
        println!(
            "  {}  outcome={}  adapter={adapter}  exit={exit}",
            contract.id, contract.outcome
        );
    }
    println!();
    println!(
        "Frozen files (never edit these; `loom stage contracts restore {stage_id}` puts them \
         back):"
    );
    for file in &record.files {
        println!("  {}  sha256={}", file.path, file.sha256);
        println!(
            "    frozen copy: {}",
            frozen_file_path(&work_dir, &stage_id, &file.path).display()
        );
    }
    Ok(())
}
