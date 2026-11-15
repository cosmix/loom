//! Node identity: settle one file's ids so no two declarations share one.
//!
//! `node_id` derives an id from path, kind and scope, so two declarations of
//! one name in one scope (`impl From<u8> for W` and `impl From<u16> for W`,
//! or a pair of `#[cfg]` twins) compute the same id. Line numbers never enter
//! an id; the suffix comes from the declaration's signature, so it survives
//! edits elsewhere in the file and, unless two signatures are identical, does
//! not depend on declaration order.

use std::collections::{BTreeMap, HashMap};

/// Final id and `symbol_key` for every `(base id, signature)`, in input order.
///
/// A base id no other definition of the file shares is kept, with an empty
/// key. Every member of a group sharing a base id becomes `{base}@{sig8}`,
/// where `sig8` is the first 8 hex digits of sha256 over its signature with
/// whitespace runs collapsed; members whose signatures still collide append
/// `.{n}`, 1-based in source order. A suffixed node's key is its base id.
pub(super) fn disambiguate(definitions: &[(String, &str)]) -> Vec<(String, String)> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, (base, _)) in definitions.iter().enumerate() {
        groups.entry(base.as_str()).or_default().push(index);
    }

    let mut settled: Vec<(String, String)> = definitions
        .iter()
        .map(|(base, _)| (base.clone(), String::new()))
        .collect();
    for members in groups.into_values().filter(|members| members.len() > 1) {
        let suffixed: Vec<String> = members
            .iter()
            .map(|&index| {
                let (base, signature) = &definitions[index];
                format!("{base}@{}", sig8(signature))
            })
            .collect();
        let mut twins: HashMap<&str, usize> = HashMap::new();
        for id in &suffixed {
            *twins.entry(id.as_str()).or_insert(0) += 1;
        }
        // Running 1-based ordinal per suffixed id, in source order.
        let mut ordinals: HashMap<&str, usize> = HashMap::new();
        for (id, &index) in suffixed.iter().zip(&members) {
            let ordinal = ordinals.entry(id.as_str()).or_insert(0);
            *ordinal += 1;
            let final_id = if twins[id.as_str()] > 1 {
                format!("{id}.{ordinal}")
            } else {
                id.clone()
            };
            settled[index] = (final_id, definitions[index].0.clone());
        }
    }
    settled
}

/// First 8 hex digits of sha256 over `signature`, whitespace runs collapsed to
/// one space.
fn sig8(signature: &str) -> String {
    let collapsed = signature.split_whitespace().collect::<Vec<_>>().join(" ");
    let digest = crate::utils::sha256_hex(collapsed.as_bytes());
    digest[..8].to_string()
}

#[cfg(test)]
#[test]
fn signature_digest_is_stable() {
    assert_eq!(sig8("fn  example()"), "ae17070f");
}
