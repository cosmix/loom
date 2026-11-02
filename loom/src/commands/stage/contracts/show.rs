//! `loom stage contracts show`: print a stage's freeze record and where its
//! frozen copies are kept.

use anyhow::Result;

use crate::verify::contracts::store::{frozen_file_path_for_record, load_freeze};

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
        let file = contract.file.as_deref().unwrap_or("unrecorded");
        let cwd = contract.cwd.as_deref().unwrap_or("unrecorded");
        println!(
            "  {}  outcome={}  adapter={adapter}  exit={exit}  file={file}  cwd={cwd}",
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
        println!("{}", frozen_copy_line(&work_dir, &record, &file.path));
    }
    Ok(())
}

fn frozen_copy_line(
    work_dir: &std::path::Path,
    record: &crate::verify::contracts::store::FreezeRecord,
    path: &str,
) -> String {
    format!(
        "    frozen copy: {}",
        frozen_file_path_for_record(work_dir, record, path).display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::contracts::store::{self, FreezeRecord, FrozenFile, FREEZE_RECORD_VERSION};

    #[test]
    fn show_names_the_copy_in_the_loaded_generation() {
        let temp = tempfile::tempdir().unwrap();
        let record = FreezeRecord {
            version: FREEZE_RECORD_VERSION,
            stage_id: "s1".into(),
            session_id: "session".into(),
            frozen_at: chrono::Utc::now(),
            base: "base".into(),
            files: vec![FrozenFile {
                path: "test.rs".into(),
                sha256: crate::relay::sha256_hex(b"old"),
            }],
            contracts: Vec::new(),
        };
        store::write_freeze(temp.path(), &record, &[("test.rs".into(), b"old".to_vec())]).unwrap();
        store::refreeze_set(
            temp.path(),
            "s1",
            &[("test.rs".into(), b"new".to_vec())],
            &[],
        )
        .unwrap();
        let current = load_freeze(temp.path(), "s1").unwrap().unwrap();
        let line = frozen_copy_line(temp.path(), &current, "test.rs");
        let path = frozen_file_path_for_record(temp.path(), &current, "test.rs");
        assert!(line.contains(&path.display().to_string()));
        assert_eq!(std::fs::read(path).unwrap(), b"new");
        assert_ne!(line, frozen_copy_line(temp.path(), &record, "test.rs"));
    }
}
