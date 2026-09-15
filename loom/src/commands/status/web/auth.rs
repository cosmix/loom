//! One process-wide authentication token, minted whenever the dashboard
//! either binds off loopback or opts into browser terminals, and the cookie
//! bootstrap that exchanges a query token for it.
//!
//! [`super::TerminalLane`] mints and checks its own copy of this same shape
//! for the terminal WebSocket lane; [`Auth`] backs the dashboard-wide
//! equivalent that remote mode requires on every other route. The two
//! cookies carry the same name and, whenever both exist, the same value -
//! `ServeOptions` requires `terminal_token` and `dashboard_token` to agree -
//! so a browser holding either has effectively satisfied both.

use anyhow::{ensure, Result};

use super::terminal::token;

/// A minted 64-lowercase-hex-character process token, port-scoped into a
/// cookie name so two dashboards on the same host never collide.
#[derive(Debug, Clone)]
pub(super) struct Auth {
    token: String,
    cookie_name: String,
}

impl Auth {
    pub(super) fn new(token: String, port: u16) -> Self {
        Self {
            cookie_name: token::cookie_name(port),
            token,
        }
    }

    /// Exchange a query-string token for the port cookie, or refuse it.
    ///
    /// `None` when the query carries no `token` at all - the caller's cue
    /// that this is not a bootstrap request, not a refusal of one.
    pub(super) fn bootstrap_cookie(&self, query: Option<&str>) -> Option<Option<String>> {
        let presented = token::query_token(query)?;
        Some(token::matches(Some(presented), &self.token).then(|| {
            format!(
                "{}={presented}; Path=/; HttpOnly; SameSite=Strict",
                self.cookie_name
            )
        }))
    }

    pub(super) fn cookie_matches(&self, cookie: Option<&str>) -> bool {
        token::matches(token::cookie_token(cookie, &self.cookie_name), &self.token)
    }
}

/// Reject anything but exactly 64 lowercase hexadecimal characters: the shape
/// [`token::mint`] produces, and the only shape a caller-supplied token (an
/// end-to-end fixture, say) may honestly claim to be a process secret.
fn validate_token_format(token: &str) -> Result<()> {
    ensure!(
        token.len() == 64
            && token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "process token must be exactly 64 lowercase hexadecimal characters"
    );
    Ok(())
}

/// Resolve one process token from the two `ServeOptions` fields, requiring
/// agreement when both are present and validating whichever is used.
///
/// Neither field is trusted merely because it is `Some`: a caller that
/// constructs `ServeOptions` directly - a test fixture, or a future public
/// caller of [`super::serve`] - gets the same format check a minted token
/// already satisfies by construction.
pub(super) fn resolve_process_token(
    dashboard_token: Option<&str>,
    terminal_token: Option<&str>,
) -> Result<Option<String>> {
    match (dashboard_token, terminal_token) {
        (Some(dashboard), Some(terminal)) => {
            ensure!(
                dashboard == terminal,
                "dashboard_token and terminal_token must match when both are set"
            );
            validate_token_format(dashboard)?;
            Ok(Some(dashboard.to_owned()))
        }
        (Some(token), None) | (None, Some(token)) => {
            validate_token_format(token)?;
            Ok(Some(token.to_owned()))
        }
        (None, None) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreeing_tokens_resolve_to_the_shared_value() {
        let token = "a".repeat(64);
        assert_eq!(
            resolve_process_token(Some(&token), Some(&token)).unwrap(),
            Some(token)
        );
    }

    #[test]
    fn disagreeing_tokens_are_rejected() {
        assert!(resolve_process_token(Some(&"a".repeat(64)), Some(&"b".repeat(64))).is_err());
    }

    #[test]
    fn malformed_tokens_are_rejected() {
        for (dashboard, terminal) in [
            (Some("short"), None),
            (Some(&*"g".repeat(64)), None),
            (Some(&*"A".repeat(64)), None),
            (None, Some("")),
        ] {
            assert!(
                resolve_process_token(dashboard, terminal).is_err(),
                "{dashboard:?}/{terminal:?} should be rejected"
            );
        }
    }

    #[test]
    fn absent_tokens_resolve_to_none() {
        assert!(resolve_process_token(None, None).unwrap().is_none());
    }

    #[test]
    fn auth_bootstrap_cookie_matches_and_mismatches() {
        let token = "a".repeat(64);
        let auth = Auth::new(token.clone(), 7373);
        assert_eq!(auth.bootstrap_cookie(None), None);
        assert_eq!(auth.bootstrap_cookie(Some("token=wrong")), Some(None));
        let cookie = auth
            .bootstrap_cookie(Some(&format!("token={token}")))
            .expect("token present")
            .expect("token matches");
        assert!(cookie.contains("HttpOnly"));
        assert!(auth.cookie_matches(Some(&format!("loom_dashboard_7373={token}"))));
        assert!(!auth.cookie_matches(Some("loom_dashboard_7373=wrong")));
        assert!(!auth.cookie_matches(None));
    }
}
