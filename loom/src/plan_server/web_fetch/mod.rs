//! The project's `WebFetch` policy (Settings → Web fetches): a mode and a domain allowlist,
//! kept in the runtime directory, read by every browser, changed only by the authoring holder.
//!
//! The bridge reads the policy from the shared cell at each fetch request, so a change applies
//! to a running session from its next request. Sessions cannot write the runtime directory's
//! root, where the file lives (`planning-session-sandbox.md`).

mod routes;
mod store;

#[cfg(test)]
mod tests;

pub use routes::routes;

use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;

use super::context::PlanContext;
use super::lifecycle::{line, safe};
use crate::plan_session::web_fetch::{FetchPolicy, FetchPolicyCell};
use crate::planning::runtime::RuntimePaths;

/// The policy the bridge reads, and where it is saved.
pub struct WebFetchSettings {
    cell: FetchPolicyCell,
    runtime: RuntimePaths,
    /// The policy's revision, held across each save so two changes never interleave their
    /// writes. It counts the changes since the server started; a browser sends the revision
    /// it read, and a change made since is refused instead of overwritten with a stale copy.
    saving: Mutex<u64>,
}

/// The policy with the revision a change to it must name.
#[derive(Serialize)]
pub struct PolicyView {
    #[serde(flatten)]
    policy: FetchPolicy,
    revision: u64,
}

/// Why a change was not applied.
#[derive(Debug)]
pub enum ReplaceError {
    /// The policy changed since the revision the browser read.
    Stale,
    Save(anyhow::Error),
}

impl WebFetchSettings {
    /// The saved policy; a missing file is Ask with an empty list, and so is a file that cannot
    /// be read or holds an invalid entry (named on the terminal): asking is the safe side.
    fn load(runtime: RuntimePaths) -> WebFetchSettings {
        let policy = match store::read(&runtime) {
            Ok(policy) => policy.unwrap_or_default(),
            Err(error) => {
                let why = safe(&format!("{error:#}"));
                line(&format!(
                    "web fetch settings unreadable, every fetch asks: {why}"
                ));
                FetchPolicy::default()
            }
        };
        let cell = FetchPolicyCell::default();
        cell.set(policy);
        WebFetchSettings {
            cell,
            runtime,
            saving: Mutex::new(0),
        }
    }

    pub fn current(&self) -> FetchPolicy {
        self.cell.get()
    }

    /// The policy and its revision, read together.
    pub fn view(&self) -> PolicyView {
        let revision = self.saving.lock().unwrap_or_else(PoisonError::into_inner);
        PolicyView {
            policy: self.cell.get(),
            revision: *revision,
        }
    }

    /// Save `policy`, then apply it: a policy that cannot be saved is not applied, and one
    /// built on a revision other than the current is not saved.
    pub fn replace(&self, policy: FetchPolicy, based_on: u64) -> Result<(), ReplaceError> {
        let mut revision = self.saving.lock().unwrap_or_else(PoisonError::into_inner);
        if *revision != based_on {
            return Err(ReplaceError::Stale);
        }
        store::write(&self.runtime, &policy).map_err(ReplaceError::Save)?;
        self.cell.set(policy);
        *revision += 1;
        Ok(())
    }

    /// List `host` from a card's "Always allow"; the Ask mode becomes Listed so the entry takes
    /// effect at once (`FetchPolicy::with_host`).
    pub fn allow_host(&self, host: &str) -> Result<(), String> {
        let mut revision = self.saving.lock().unwrap_or_else(PoisonError::into_inner);
        let policy = self.cell.get().with_host(host)?;
        if let Err(error) = store::write(&self.runtime, &policy) {
            line(&format!(
                "web fetch settings not saved: {}",
                safe(&format!("{error:#}"))
            ));
            return Err("the web fetch settings could not be saved".to_owned());
        }
        self.cell.set(policy);
        *revision += 1;
        Ok(())
    }
}

/// The installed settings, loading them on the first call (the chat's install, before the
/// bridge exists).
pub fn settings(ctx: &PlanContext) -> Arc<WebFetchSettings> {
    if let Some(settings) = ctx.extensions().get::<WebFetchSettings>() {
        return settings;
    }
    let settings = Arc::new(WebFetchSettings::load(ctx.runtime().clone()));
    ctx.extensions().insert(Arc::clone(&settings));
    settings
}

/// The cell the bridge reads at each `WebFetch` request.
pub fn policy(ctx: &PlanContext) -> FetchPolicyCell {
    settings(ctx).cell.clone()
}
