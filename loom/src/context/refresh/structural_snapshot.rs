use anyhow::Result;

use super::structural_freshness;
use crate::context::schema::Freshness;
use crate::context::store::{ContextStore, StoreState};
use crate::fs::knowledge::catalog::Catalog;

pub(super) fn catalog_freshness(
    stored: &StoreState,
    catalog: &Option<Catalog>,
    current_revision: &str,
) -> Freshness {
    let mut structural = structural_freshness(&stored.structural, current_revision);
    if !stored.structural.revision.is_empty()
        && catalog
            .as_ref()
            .is_none_or(|catalog| catalog.revision != stored.catalog_revision)
    {
        structural.stale = true;
        structural.detail =
            Some("cached catalog is missing or does not match the recorded revision".to_string());
    }
    structural
}

pub(super) fn publish(
    store: &ContextStore,
    catalog: &Catalog,
    structural: &mut Freshness,
    tolerate_write_failure: bool,
) -> Result<()> {
    if let Err(error) = store.publish_catalog(catalog, structural) {
        if !tolerate_write_failure {
            return Err(error);
        }
        eprintln!("warning: failed to publish context cache ({error}); using an in-memory catalog");
        structural.detail = Some("in-memory catalog; cache not updated".into());
    }
    Ok(())
}
