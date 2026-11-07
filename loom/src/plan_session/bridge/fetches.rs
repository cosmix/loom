//! `WebFetch` requests the standing rules answer (`web_fetch.rs`): on arrival, with a closed
//! card and no wait; and the holder's commands that grow the rules from a card ("Allow this
//! session", `"Always allow <host>"`) and take the session's grant back ("Ask again").
//!
//! Only these commands set the grant, and only the server calls them, for the browser holding
//! authoring; nothing a session writes reaches them.

use super::commands::Step;
use super::requests::{card, deny_answer, shown_input, shown_status, GONE};
use super::session::{Request, Session};
use super::{Bridge, BridgeConfig, BridgeError};
use crate::plan_session::model::{DenyReason, FetchRule, FetchView};
use crate::plan_session::permission::{Answer, Behavior};
use crate::plan_session::web_fetch::{FetchPolicy, TURNED_OFF};

const NOT_A_FETCH: &str = "only a web fetch can be allowed this way";
const NO_HOST: &str = "this fetch names no host that can be listed";

impl Bridge {
    /// The holder's "Allow this session" on an open `WebFetch` card: the session's later
    /// fetches are allowed unless Settings say Never; this card and every other open fetch card
    /// the rules now cover are answered.
    pub fn grant_fetches(&self, id: &str, request_id: &str) -> Result<(), BridgeError> {
        let policy = self.config.fetch_policy.get();
        let granted = self.with_session_io(id, |session, step| {
            session.open_fetch(request_id)?;
            session.flow.fetch_grant = true;
            session.answer_open_fetches(&policy, step);
            Ok(())
        });
        self.flush();
        io_outcome(granted)
    }

    /// "Ask again": the session's fetches show a card again, unless Settings answer them.
    pub fn revoke_fetches(&self, id: &str) -> Result<(), BridgeError> {
        let revoked = self.with_session(id, |session, _| session.flow.fetch_grant = false);
        self.flush();
        revoked.ok_or(BridgeError::NoSession)
    }

    /// The host an open `WebFetch` card's URL names, when it can go on the allowlist.
    pub fn fetch_host(&self, id: &str, request_id: &str) -> Result<String, BridgeError> {
        let inner = self.lock();
        let session = inner.sessions.get(id).ok_or(BridgeError::NoSession)?;
        let fetch = session.open_fetch(request_id)?;
        let host = fetch.host.clone().filter(|_| fetch.listable);
        host.ok_or_else(|| BridgeError::Refused(NO_HOST.to_owned()))
    }

    /// Answer every open fetch card of the session the current rules cover (after the holder
    /// listed a host from a card).
    pub fn answer_fetches(&self, id: &str) -> Result<(), BridgeError> {
        let policy = self.config.fetch_policy.get();
        let answered = self.with_session_io(id, |session, step| {
            session.answer_open_fetches(&policy, step);
            Ok(())
        });
        self.flush();
        io_outcome(answered)
    }
}

/// A command's result from [`Bridge::with_session_io`]: no session, its refusal, or the first
/// answer file that could not be written.
pub(super) fn io_outcome(
    outcome: Option<(Result<(), BridgeError>, Option<String>)>,
) -> Result<(), BridgeError> {
    match outcome {
        None => Err(BridgeError::NoSession),
        Some((Err(refused), _)) => Err(refused),
        Some((Ok(()), Some(failed))) => Err(BridgeError::Io(failed)),
        Some((Ok(()), None)) => Ok(()),
    }
}

impl Session {
    /// The rule that answers a new request before anyone is asked, with the request's fetch as
    /// that rule settles it. A recovered answer, a cut input and an ending's denial come first
    /// (`answer_on_arrival`).
    pub(super) fn arrival_rule(
        &self,
        request_id: &str,
        request: &Request,
        config: &BridgeConfig,
    ) -> Option<(FetchRule, FetchView)> {
        let fetch = request.fetch.as_ref()?;
        let recovered = self.flow.recovered.as_ref();
        let recovered = recovered.is_some_and(|answers| answers.contains_key(request_id));
        let ending = self
            .flow
            .end
            .as_ref()
            .is_some_and(|end| end.deny_new.is_some());
        if recovered || ending || shown_input(&request.input).1 {
            return None;
        }
        config
            .fetch_policy
            .get()
            .settle(fetch, self.flow.fetch_grant)
    }

    /// Show the request already answered by `rule`, write its answer and tell the machine: no
    /// pending card, so no browser is asked or pinged.
    pub(super) fn answer_by_rule_on_arrival(
        &mut self,
        request_id: String,
        mut request: Request,
        (rule, settled): (FetchRule, FetchView),
        step: &mut Step,
    ) {
        let answer = rule_answer(rule);
        request.fetch = Some(settled);
        request.open = false;
        let (status, reason) = shown_status(&answer);
        let shown = card(
            &request_id,
            &request,
            status,
            reason,
            answer.message.clone(),
        );
        let deltas = self.mapper.add_server_item(shown);
        self.emit(deltas, step);
        self.flow.requests.insert(request_id.clone(), request);
        self.reply(&request_id, answer, false, step);
    }

    /// The open `WebFetch` request `request_id`, or why a fetch command cannot act on it.
    fn open_fetch(&self, request_id: &str) -> Result<&FetchView, BridgeError> {
        let request = self.flow.requests.get(request_id);
        let request = request.filter(|request| request.open);
        let request = request.ok_or_else(|| BridgeError::Refused(GONE.to_owned()))?;
        let fetch = request.fetch.as_ref();
        fetch.ok_or_else(|| BridgeError::Refused(NOT_A_FETCH.to_owned()))
    }

    /// Close every open fetch card `policy` and the session's grant now answer.
    fn answer_open_fetches(&mut self, policy: &FetchPolicy, step: &mut Step) {
        let granted = self.flow.fetch_grant;
        let due: Vec<(String, FetchRule, FetchView)> = self
            .flow
            .requests
            .iter()
            .filter(|(_, request)| request.open)
            .filter_map(|(id, request)| {
                let (rule, settled) = policy.settle(request.fetch.as_ref()?, granted)?;
                Some((id.clone(), rule, settled))
            })
            .collect();
        for (request_id, rule, settled) in due {
            if let Some(request) = self.flow.requests.get_mut(&request_id) {
                request.fetch = Some(settled);
            }
            self.answer(&request_id, rule_answer(rule), false, step);
        }
    }
}

/// Never denies with the Settings message; every other rule allows.
fn rule_answer(rule: FetchRule) -> Answer {
    match rule {
        FetchRule::Never => deny_answer(DenyReason::Settings, TURNED_OFF),
        FetchRule::Always | FetchRule::Listed | FetchRule::Session => Answer {
            behavior: Behavior::Allow,
            message: None,
            reason: None,
        },
    }
}
