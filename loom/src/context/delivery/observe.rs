//! Strict, best-effort observation for telemetry; normal retrieval remains tolerant.

use super::*;

pub(crate) fn observe_deliveries(
    work_dir: &Path,
    plan: &str,
    stage: &str,
) -> Result<Vec<DeliveryRecord>> {
    let root = fs::canonicalize(work_dir)?;
    let root_fd = safe_open_dirfd(&root)?;
    let mut records = Vec::new();
    for dir in [
        delivery_dir(work_dir, plan, stage),
        worker_delivery_dir(work_dir, plan, stage),
    ] {
        let entries = match list_dir_no_follow(&root_fd, dir.strip_prefix(work_dir)?) {
            Ok(entries) => entries,
            Err(error) if missing(&error) => continue,
            Err(error) => return Err(error),
        };
        for (name, kind) in entries {
            let path = dir.strip_prefix(work_dir)?.join(name);
            if kind != EntryKind::File
                || path.extension().and_then(|ext| ext.to_str()) != Some("json")
            {
                continue;
            }
            let bytes = read_bounded_at(&root_fd, &path, DELIVERY_RECORD_MAX_BYTES)?;
            records.push(serde_json::from_slice::<DeliveryRecord>(&bytes)?);
        }
    }
    records.sort_by(|left, right| {
        left.recipient_id
            .cmp(&right.recipient_id)
            .then_with(|| left.launch_id.cmp(&right.launch_id))
    });
    Ok(records)
}

fn missing(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            || cause
                .downcast_ref::<nix::errno::Errno>()
                .is_some_and(|e| *e == nix::errno::Errno::ENOENT)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_and_corrupt_records_are_distinct() {
        let temp = tempfile::tempdir().unwrap();
        assert!(observe_deliveries(temp.path(), "plan", "stage")
            .unwrap()
            .is_empty());
        let dir = delivery_dir(temp.path(), "plan", "stage");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("bad.json"), "not json").unwrap();
        assert!(observe_deliveries(temp.path(), "plan", "stage").is_err());
        assert!(load_deliveries(temp.path(), "plan", "stage")
            .unwrap()
            .is_empty());
    }
}
