//! What a session belongs to, named after its start: a kickoff learns it only at confirmation.

use super::{Bridge, BridgeError};

impl Bridge {
    /// Name the feature or plan a running session belongs to and publish its view. The driver
    /// rewrites `session.json` on its next step, so a recovered session stays with it.
    pub fn set_owner(
        &self,
        id: &str,
        feature: Option<String>,
        plan: Option<String>,
    ) -> Result<(), BridgeError> {
        let set = self.with_session(id, |session, _| {
            if !session.active() {
                return Err(BridgeError::NotRunning);
            }
            session.record.feature = feature;
            session.record.plan = plan;
            session.record_dirty = true;
            Ok(())
        });
        self.flush();
        set.unwrap_or(Err(BridgeError::NoSession))
    }

    /// The feature and plan of the chat that ended last; both `None` once a new one started.
    pub fn last_chat_owner(&self) -> (Option<String>, Option<String>) {
        let inner = self.lock();
        let ended = inner.previous.as_ref().map(|ended| &ended.session);
        ended
            .map(|session| (session.feature.clone(), session.plan.clone()))
            .unwrap_or_default()
    }
}
