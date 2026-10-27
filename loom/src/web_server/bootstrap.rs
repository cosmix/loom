//! Exchanging a bootstrap query token for a server's access cookie.
//!
//! Operates on a request head that was only peeked, so a router runs it from
//! [`super::Router::upgrade`], ahead of reading the request.

use std::net::{SocketAddr, TcpStream};

use super::access::{AccessPolicy, OriginRequirement};
use super::auth::Auth;
use super::connection::fail;
use super::http::{self, RequestHead};

/// Handle the cookie bootstrap, `GET`/`HEAD /?token=`, against `auth`'s token
/// and cookie. Returns true when the connection is finished: redirected to `/`
/// with the cookie set, or refused with a 403. Returns false when this is not
/// a bootstrap request at all - another path or method, no `token` in the
/// query, or no `auth` to exchange it for - so the caller routes it normally.
///
/// A remote policy validates `Origin` here, ahead of the token exchange: an
/// absent `Origin` is fine (this is the same navigation a bootstrap link
/// produces), but a supplied foreign one means the request did not come from
/// typing the tokenized URL into a browser, and must be refused before the
/// token is even looked at.
pub(crate) fn bootstrap_cookie(
    stream: &mut TcpStream,
    head: &RequestHead,
    policy: &AccessPolicy,
    local: SocketAddr,
    auth: Option<&Auth>,
) -> bool {
    if head.path != "/" || !matches!(head.method.as_str(), "GET" | "HEAD") {
        return false;
    }
    if policy.is_remote() && !policy.origin_allowed(local, head, OriginRequirement::Optional) {
        fail(stream, 403, "Forbidden", b"origin not allowed");
        return true;
    }
    let query = head.query.as_deref();
    let Some(cookie) = auth.and_then(|auth| auth.bootstrap_cookie(query)) else {
        return false;
    };
    let Some(cookie) = cookie else {
        fail(stream, 403, "Forbidden", b"token not accepted");
        return true;
    };
    let _ = http::write_redirect(stream, "/", Some(&cookie), &[]);
    true
}
