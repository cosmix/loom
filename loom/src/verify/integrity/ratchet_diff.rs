//! Ratchet-file loosening rule: whether a changed ratchet file allows more than its base did.

use std::collections::BTreeMap;

/// True when a changed ratchet file must raise `TI-ratchet`.
///
/// Values are ceilings: larger values permit more. A floor such as a coverage
/// minimum must retain a non-integer last token (for example `80%`) so changes
/// to it remain unparsable and raise an event. An absent file also raises.
pub(super) fn ratchet_loosens(base: Option<&[u8]>, head: Option<&[u8]>) -> bool {
    let Some((base, head)) = base.and_then(parse).zip(head.and_then(parse)) else {
        return true;
    };
    head.iter()
        .any(|(key, value)| base.get(key).is_none_or(|previous| value > previous))
}

fn parse(bytes: &[u8]) -> Option<BTreeMap<String, u64>> {
    let mut entries = BTreeMap::new();
    for line in std::str::from_utf8(bytes).ok()?.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.rsplit_once(char::is_whitespace)?;
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let key = key.split_whitespace().collect::<Vec<_>>().join(" ");
        if key.is_empty() || entries.insert(key, value.parse().ok()?).is_some() {
            return None;
        }
    }
    Some(entries)
}

#[cfg(test)]
#[path = "ratchet_diff_tests.rs"]
mod tests;
