//! The standing rules that answer a `WebFetch` permission request without a card: the project's
//! fetch policy (Settings: a mode and a domain allowlist) and a session's own grant.
//!
//! Precedence, first match wins: Never denies; Always allows; Listed allows a host on the list;
//! the session's grant allows; anything else shows the card. A URL whose host cannot be read,
//! is an IP address, `localhost` (or under it) or a single label is never on the list and is
//! never allowed by Always or the session's grant: a fetch runs in Claude Code's own process,
//! outside the sandbox, where such a name reaches cloud metadata endpoints and local services,
//! so only the holder's click on its card allows it.

use std::sync::{Arc, PoisonError, RwLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{Host, Url};

use super::model::{FetchRule, FetchView};
use crate::web_server::http::MAX_BODY_BYTES;

/// The tool these rules answer for; every other tool keeps its card.
pub const FETCH_TOOL: &str = "WebFetch";
/// The deny message of a fetch the Never mode answers.
pub const TURNED_OFF: &str = "Web fetches are turned off in Settings.";
/// At most this many allowlist entries.
pub const MAX_DOMAINS: usize = 100;
/// The most a policy can serialize to. `POST /api/web-fetch` takes a JSON body of at most
/// `MAX_BODY_BYTES`, so a policy past that could be listed but never sent back, locking
/// Settings out (even a switch to Never); half leaves room for the revision field and for
/// escaping in the sent form.
pub const MAX_POLICY_BYTES: usize = MAX_BODY_BYTES / 2;
/// The longest URL a card shows; a longer one ends in `…` (the card's input shows it whole).
const MAX_SHOWN_URL_CHARS: usize = 2048;
/// The longest DNS name.
const MAX_HOST_CHARS: usize = 253;
/// How much of a refused entry an error message repeats.
const SHOWN_CHARS: usize = 80;

/// What the server answers a fetch request with before anyone is asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchMode {
    /// Every fetch shows a card (a session grant still applies).
    #[default]
    Ask,
    /// A fetch of a listed host is allowed; any other shows a card.
    Listed,
    /// Every fetch is allowed.
    Always,
    /// Every fetch is denied, a session grant included.
    Never,
}

/// The project's fetch policy. `domains` are validated, lowercase, IDN in punycode; the list
/// is kept whatever the mode.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FetchPolicy {
    pub mode: FetchMode,
    pub domains: Vec<String>,
}

impl FetchPolicy {
    /// `mode` with `entries` validated, duplicates dropped in order; the first invalid entry
    /// is the error, and so is a list past `MAX_DOMAINS` entries or `MAX_POLICY_BYTES` saved.
    pub fn validated(mode: FetchMode, entries: &[String]) -> Result<FetchPolicy, String> {
        let mut domains: Vec<String> = Vec::new();
        for entry in entries {
            let domain = validate_domain(entry)?;
            if domains.contains(&domain) {
                continue;
            }
            if domains.len() == MAX_DOMAINS {
                return Err(format!("at most {MAX_DOMAINS} domains can be listed"));
            }
            domains.push(domain);
        }
        let policy = FetchPolicy { mode, domains };
        let saved = serde_json::to_vec(&policy).map_or(usize::MAX, |bytes| bytes.len());
        if saved > MAX_POLICY_BYTES {
            return Err(format!(
                "the list is too long to save ({MAX_POLICY_BYTES} bytes at most); remove a domain first"
            ));
        }
        Ok(policy)
    }

    /// `fetch` with the rule that answers it without a card, and the listed domain that matched,
    /// in precedence order; `granted` is the session's own grant.
    pub fn settle(&self, fetch: &FetchView, granted: bool) -> Option<(FetchRule, FetchView)> {
        let rule = self.rule(fetch, granted)?;
        let mut settled = fetch.clone();
        settled.rule = Some(rule);
        if rule == FetchRule::Listed {
            settled.matched = fetch.host.as_deref().and_then(|host| self.matching(host));
        }
        Some((rule, settled))
    }

    fn rule(&self, fetch: &FetchView, granted: bool) -> Option<FetchRule> {
        // Always and the session's grant never reach a name that is local or unreadable.
        let open = fetch.host.as_deref().is_some_and(|host| !local_name(host));
        match self.mode {
            FetchMode::Never => return Some(FetchRule::Never),
            FetchMode::Always if open => return Some(FetchRule::Always),
            FetchMode::Listed if self.lists(fetch.host.as_deref()) => {
                return Some(FetchRule::Listed)
            }
            FetchMode::Always | FetchMode::Listed | FetchMode::Ask => {}
        }
        (granted && open).then_some(FetchRule::Session)
    }

    /// `host` is a listed domain or a subdomain of one.
    pub fn lists(&self, host: Option<&str>) -> bool {
        host.is_some_and(|host| self.matching(host).is_some())
    }

    /// The listed domain that covers `host`.
    fn matching(&self, host: &str) -> Option<String> {
        let found = self.domains.iter().find(|domain| covers(domain, host));
        found.cloned()
    }

    /// The policy with `host` listed, and the Ask mode moved to Listed so the entry takes
    /// effect; Listed, Always and Never keep their mode. A host that would grow the list past
    /// `MAX_DOMAINS` or `MAX_POLICY_BYTES` is refused, as in `validated`.
    pub fn with_host(&self, host: &str) -> Result<FetchPolicy, String> {
        let mut entries = self.domains.clone();
        entries.push(host.to_owned());
        let mode = match self.mode {
            FetchMode::Ask => FetchMode::Listed,
            mode => mode,
        };
        FetchPolicy::validated(mode, &entries)
    }
}

/// `domain` itself, or a name ending in `.domain`: `docs.rs` covers `x.docs.rs` and never
/// `evildocs.rs`.
fn covers(domain: &str, host: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|rest| rest.ends_with('.'))
}

/// The policy the bridge reads at each request and the settings route replaces.
#[derive(Clone, Default)]
pub struct FetchPolicyCell(Arc<RwLock<FetchPolicy>>);

impl FetchPolicyCell {
    pub fn get(&self) -> FetchPolicy {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, policy: FetchPolicy) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = policy;
    }
}

/// What a `WebFetch` request's card shows of its URL; `rule` is set when one answers it.
pub fn fetch_view(input: &Value) -> FetchView {
    let url = input.get("url").and_then(Value::as_str).unwrap_or("");
    let host = url_host(url);
    let listable = host
        .as_deref()
        .is_some_and(|host| validate_domain(host).is_ok());
    FetchView {
        url: shown_url(url),
        host,
        listable,
        matched: None,
        rule: None,
    }
}

/// The URL as the fetch will read it, the host in punycode so a look-alike shows as one, cut
/// at `MAX_SHOWN_URL_CHARS`; text that is not a URL shows as it is.
fn shown_url(url: &str) -> String {
    let shown = Url::parse(url).map_or_else(|_| url.to_owned(), String::from);
    if shown.chars().nth(MAX_SHOWN_URL_CHARS).is_none() {
        return shown;
    }
    let mut cut: String = shown.chars().take(MAX_SHOWN_URL_CHARS).collect();
    cut.push('…');
    cut
}

/// A name that reaches this machine or its own network instead of the internet: a single
/// label, `localhost` or any name under it. (IP addresses have no host here at all.)
fn local_name(host: &str) -> bool {
    !host.contains('.') || host == "localhost" || host.ends_with(".localhost")
}

/// The host name of an `http` or `https` URL, lowercase and in punycode, without a trailing
/// dot; `None` for an IP address (any form the URL parser normalises to one), another scheme
/// or a URL that does not parse.
pub fn url_host(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    match parsed.host()? {
        Host::Domain(domain) => plain_name(domain),
        Host::Ipv4(_) | Host::Ipv6(_) => None,
    }
}

/// An allowlist entry: a bare host name of two labels or more, lowercase and in punycode.
pub fn validate_domain(entry: &str) -> Result<String, String> {
    let entry = entry.trim();
    let shown: String = entry.chars().take(SHOWN_CHARS).collect();
    let refuse = |why: &str| Err(format!("\"{shown}\" {why}"));
    if entry.is_empty() {
        return Err("an empty domain cannot be listed".to_owned());
    }
    // Any ASCII outside letters, digits, `-` and `.` is a scheme, port, path, userinfo,
    // wildcard, bracketed address or escape; IDN letters pass through to the parser.
    let plain = |c: char| !c.is_ascii() || c.is_ascii_alphanumeric() || c == '-' || c == '.';
    if !entry
        .chars()
        .all(|c| plain(c) && !c.is_control() && !c.is_whitespace())
    {
        return refuse("is not a bare host name: no scheme, port, path or wildcard");
    }
    let Ok(parsed) = Url::parse(&format!("https://{entry}/")) else {
        return refuse("is not a valid host name");
    };
    let Some(Host::Domain(domain)) = parsed.host() else {
        return refuse("is an IP address; list a host name");
    };
    match plain_name(domain) {
        Some(name) if !name.contains('.') => refuse("needs at least two labels, as in docs.rs"),
        Some(name) if local_name(&name) => refuse("names this machine, not a host to list"),
        Some(name) => Ok(name),
        None => refuse("is not a valid host name"),
    }
}

/// `domain` without one trailing dot, when every label is non-empty and the name fits DNS.
fn plain_name(domain: &str) -> Option<String> {
    let name = domain.strip_suffix('.').unwrap_or(domain);
    let labels_ok = name.split('.').all(|label| !label.is_empty());
    (labels_ok && name.len() <= MAX_HOST_CHARS).then(|| name.to_ascii_lowercase())
}

#[cfg(test)]
#[path = "web_fetch_tests.rs"]
mod tests;
