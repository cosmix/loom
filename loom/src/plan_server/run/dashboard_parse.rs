//! Parse the dashboard child's startup output and redact its bearer token.

use std::net::SocketAddr;

/// Startup lines the dashboard may print before its URL; later lines are never read.
pub(super) const STARTUP_LINES: usize = 20;

/// Return the dashboard URL printed in the first twenty startup lines.
///
/// A `this machine` (or loopback) line wins. A remote bind to one concrete address prints only
/// `another machine`, for that address, so that line is taken when it names the bound address.
/// A wildcard bind advertises every interface, none of which is preferred over another.
pub fn parse_dashboard_startup(lines: &[&str]) -> Option<String> {
    let lines = &lines[..lines.len().min(STARTUP_LINES)];
    local_url(lines).or_else(|| concrete_bind_url(lines))
}

fn local_url(lines: &[&str]) -> Option<String> {
    lines.iter().find_map(|line| {
        let content = line
            .strip_prefix("loom dashboard: ")
            .or_else(|| line.strip_prefix("  bootstrap from this machine at: "))?;
        Some(address_url(content)?.0.to_owned())
    })
}

fn concrete_bind_url(lines: &[&str]) -> Option<String> {
    let bound = lines.iter().find_map(|line| {
        let header = line.strip_prefix("loom dashboard listening on ")?;
        header.split_whitespace().next()?.parse::<SocketAddr>().ok()
    })?;
    if bound.ip().is_unspecified() {
        return None;
    }
    lines.iter().find_map(|line| {
        let content = line.strip_prefix("  bootstrap from another machine at: ")?;
        let (url, address) = address_url(content)?;
        (address == bound).then(|| url.to_owned())
    })
}

/// The first word of `content` when it is an `http://<socket address>/` URL, with that address.
fn address_url(content: &str) -> Option<(&str, SocketAddr)> {
    let url = content.split_whitespace().next()?;
    let (address, _) = url.strip_prefix("http://")?.split_once('/')?;
    Some((url, address.parse().ok()?))
}

/// Remove every `token` query parameter without re-encoding the remaining URL.
pub fn strip_token(url: &str) -> String {
    let (without_fragment, fragment) = url
        .split_once('#')
        .map_or((url, None), |(base, fragment)| (base, Some(fragment)));
    let Some((base, query)) = without_fragment.split_once('?') else {
        return url.to_owned();
    };
    let query = query
        .split('&')
        .filter(|parameter| !parameter.is_empty() && parameter.split('=').next() != Some("token"))
        .collect::<Vec<_>>()
        .join("&");
    let mut result = base.to_owned();
    if !query.is_empty() {
        result.push('?');
        result.push_str(&query);
    }
    if let Some(fragment) = fragment {
        result.push('#');
        result.push_str(fragment);
    }
    result
}

#[cfg(test)]
mod tests;
