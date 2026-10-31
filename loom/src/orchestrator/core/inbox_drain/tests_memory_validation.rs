use super::*;
use crate::fs::memory::{MemoryEntryType, Receipt, ReceiptOutcome};

#[test]
fn json_memory_requests_refuse_entries_the_journal_cannot_read() {
    let dir = tempfile::tempdir().unwrap();
    let good = MemoryEntry::new(MemoryEntryType::Note, "valid".into());
    let receipt = Receipt {
        event_id: good.id.clone(),
        outcome: ReceiptOutcome::Implemented,
        target: None,
    };
    let mut bad_id = good.clone();
    bad_id.id = "bad".into();
    let missing = MemoryEntry::new(MemoryEntryType::Receipt, "missing".into());
    let mut extra = good.clone();
    extra.receipt = Some(receipt);
    let blank = MemoryEntry::new(MemoryEntryType::Note, "  \n".into());
    for original in [bad_id, missing, extra, blank] {
        let payload = serde_json::to_value(original).unwrap();
        let entry: MemoryEntry = serde_json::from_value(payload).unwrap();
        assert!(matches!(
            memory(dir.path(), "stage", &entry),
            Settle::Refused(_)
        ));
    }
    assert!(matches!(
        memory(dir.path(), "stage", &good),
        Settle::Applied(None)
    ));
    let journal = crate::fs::memory::read_journal(dir.path(), "stage").unwrap();
    assert_eq!(journal.entries.len(), 1);
    assert_eq!(journal.entries[0].id, good.id);
}
