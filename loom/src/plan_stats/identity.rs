//! The identity of one initialized plan run, kept in the `[plan]` table.
//!
//! `loom init` stamps the UTC initialization time, the source file's slug and a
//! fresh run UUID. They fix the archive name for the whole run: a rename of the
//! plan file, a restart, or a different observation time never moves it, and two
//! initializations of the same plan never share it. A run initialized before the
//! keys existed gets them once, under the config lock, and keeps them on retry.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use toml_edit::{value, Item, Table};
use uuid::Uuid;

use crate::fs::work_dir::{load_config, update_config};

const INITIALIZED_AT_KEY: &str = "stats_initialized_at";
const SLUG_KEY: &str = "stats_source_slug";
const RUN_ID_KEY: &str = "stats_run_id";

/// Longest source slug kept in an archive name.
const MAX_SLUG_LEN: usize = 48;
/// Slug of a source file whose name leaves nothing usable.
const FALLBACK_SLUG: &str = "plan";
/// Prefixes the plan lifecycle adds to a source file's name.
const LIFECYCLE_PREFIXES: [&str; 2] = ["IN_PROGRESS-", "DONE-"];

/// What names a run's stats; see the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunIdentity {
    pub initialized_at: DateTime<Utc>,
    slug: String,
    run_id: Uuid,
}

impl RunIdentity {
    #[cfg(test)]
    pub fn new(initialized_at: DateTime<Utc>, slug: &str, run_id: Uuid) -> Self {
        Self {
            initialized_at,
            slug: slug.to_string(),
            run_id,
        }
    }

    /// `<init UTC %Y%m%dT%H%M%SZ>-<slug>-<run uuid>`.
    pub fn plan_id(&self) -> String {
        format!(
            "{}-{}-{}",
            self.initialized_at.format("%Y%m%dT%H%M%SZ"),
            self.slug,
            self.run_id.hyphenated()
        )
    }
}

/// Stamp a new run into the `[plan]` table `loom init` is about to write.
pub fn stamp_new_run(plan: &mut Table, source_path: &str) {
    set_initialized_at(plan, Utc::now());
    plan[SLUG_KEY] = value(source_slug(source_path));
    plan[RUN_ID_KEY] = value(Uuid::new_v4().hyphenated().to_string());
}

fn set_initialized_at(plan: &mut Table, at: DateTime<Utc>) {
    plan[INITIALIZED_AT_KEY] = value(at.to_rfc3339_opts(SecondsFormat::Secs, true));
}

/// The slug of a plan source path: the file stem without the lifecycle prefix,
/// lowercased to `[a-z0-9-]`, cut to [`MAX_SLUG_LEN`]; `plan` when nothing is left.
fn source_slug(source_path: &str) -> String {
    let file = source_path.rsplit('/').next().unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let stem = LIFECYCLE_PREFIXES
        .iter()
        .find_map(|prefix| stem.strip_prefix(prefix))
        .unwrap_or(stem);
    let mut slug = String::new();
    for ch in stem.chars().map(|ch| ch.to_ascii_lowercase()) {
        let ch = if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            ch
        } else {
            '-'
        };
        if ch != '-' || !(slug.is_empty() || slug.ends_with('-')) {
            slug.push(ch);
        }
    }
    slug.truncate(MAX_SLUG_LEN);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        FALLBACK_SLUG.to_string()
    } else {
        slug.to_string()
    }
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= MAX_SLUG_LEN
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn parse_at(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

/// The identity held by the `[plan]` keys `get` reads, when all three are valid.
fn parse_identity<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Option<RunIdentity> {
    Some(RunIdentity {
        initialized_at: get(INITIALIZED_AT_KEY).and_then(parse_at)?,
        slug: get(SLUG_KEY).filter(|slug| valid_slug(slug))?.to_string(),
        run_id: get(RUN_ID_KEY).and_then(|id| Uuid::parse_str(id).ok())?,
    })
}

/// The run identity `loom init` recorded, or `None` when the state holds no
/// plan or a plan without a complete identity.
pub(crate) fn read(work_dir: &Path) -> Result<Option<RunIdentity>> {
    let config = load_config(work_dir)?;
    Ok(config
        .as_ref()
        .and_then(|config| parse_identity(|key| config.get_plan_str(key))))
}

/// The run identity, assigning every missing or malformed key once under the
/// config lock. A later call, or a retry after a crash, reads what the first
/// one wrote.
pub(crate) fn ensure(work_dir: &Path) -> Result<RunIdentity> {
    if let Some(identity) = read(work_dir)? {
        return Ok(identity);
    }
    update_config(work_dir, |doc| {
        let plan = doc
            .get_mut("plan")
            .and_then(Item::as_table_mut)
            .context("the config has no [plan] table; run loom init first")?;
        fill_missing(plan);
        Ok(())
    })?;
    read(work_dir)?.context("the run identity is still incomplete after it was assigned")
}

/// Assign the keys that are absent or unreadable, leaving valid ones alone.
fn fill_missing(plan: &mut Table) {
    let text = |plan: &Table, key: &str| plan.get(key).and_then(Item::as_str).map(str::to_string);
    if text(plan, INITIALIZED_AT_KEY)
        .and_then(|at| parse_at(&at))
        .is_none()
    {
        set_initialized_at(plan, Utc::now());
    }
    if !text(plan, SLUG_KEY).is_some_and(|slug| valid_slug(&slug)) {
        let source = text(plan, "source_path").unwrap_or_default();
        plan[SLUG_KEY] = value(source_slug(&source));
    }
    if !text(plan, RUN_ID_KEY).is_some_and(|id| Uuid::parse_str(&id).is_ok()) {
        plan[RUN_ID_KEY] = value(Uuid::new_v4().hyphenated().to_string());
    }
}

#[cfg(test)]
#[path = "identity_tests.rs"]
mod tests;
