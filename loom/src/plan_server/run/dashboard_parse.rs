//! Parse the dashboard child's startup output and redact its bearer token.

/// Return the first local dashboard URL in the first twenty startup lines.
/// Remote startup advertises other interfaces too; only `this machine` is used.
pub fn parse_dashboard_startup(lines: &[&str]) -> Option<String> {
    lines.iter().take(20).find_map(|line| {
        let content = line
            .strip_prefix("loom dashboard: ")
            .or_else(|| line.strip_prefix("  bootstrap from this machine at: "))?;
        let url = content.split_whitespace().next()?;
        let (address, _) = url.strip_prefix("http://")?.split_once('/')?;
        address.parse::<std::net::SocketAddr>().ok()?;
        Some(url.to_owned())
    })
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
