//! The standalone trusted installer carries release identity for all consumers.
use anyhow::{bail, Result};

const INSTALLER: &str = include_str!("../../../../install.sh");

pub(super) fn value(name: &str) -> &'static str {
    let prefix = format!("{name}=\"");
    INSTALLER
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .and_then(|value| value.strip_suffix('"'))
        .expect("release contract must contain a quoted scalar")
}

pub(super) fn asset_for_target(target: &str) -> Result<&'static str> {
    let assets = INSTALLER
        .split_once("RELEASE_ASSETS=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .expect("release contract must contain assets")
        .0;
    for line in assets.lines() {
        let mut fields = line.split(',');
        if fields.next() == Some(target) {
            if let Some(asset) = fields.nth(2) {
                return Ok(asset);
            }
        }
    }
    bail!("No release asset for this platform ({target})")
}
