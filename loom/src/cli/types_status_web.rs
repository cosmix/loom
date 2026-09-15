use std::net::{IpAddr, Ipv4Addr};

use clap::Args;

/// Dashboard-only options kept separate to preserve the main CLI type's size budget.
#[derive(Args)]
pub struct StatusWebArgs {
    /// Serve the live dashboard (omit PORT to find a port from 7373; 0 picks a free port).
    /// Binds 127.0.0.1 unless --host names a different address.
    #[arg(
        long,
        value_name = "PORT",
        num_args = 0..=1,
        conflicts_with_all = ["live", "compact", "verbose"]
    )]
    pub web: Option<Option<u16>>,
    /// Let the dashboard open live stage terminals (tmux backend only). Prints a tokenized URL.
    #[arg(long, requires = "web")]
    pub terminals: bool,
    /// Bind the dashboard to this address instead of 127.0.0.1: an IPv4/IPv6
    /// literal, or the exact alias `localhost`. Binding off loopback serves
    /// plain, unencrypted HTTP and requires the printed token cookie on
    /// every route.
    #[arg(long, requires = "web", value_parser = parse_web_host)]
    pub host: Option<IpAddr>,
}

impl StatusWebArgs {
    /// The bind host to serve on: `--host`, or the loopback default.
    pub fn resolved_host(&self) -> IpAddr {
        self.host.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }
}

/// Parse `--host`: an IPv4/IPv6 literal, or the exact alias `localhost`.
///
/// Rejects DNS names, whitespace, URLs, and any authority carrying a port -
/// `--web` already owns the port - so a malformed value fails at parse time
/// rather than resolving to something the operator did not ask for.
pub fn parse_web_host(raw: &str) -> Result<IpAddr, String> {
    if raw == "localhost" {
        return Ok(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    raw.parse::<IpAddr>()
        .map_err(|_| format!("{raw:?} is not an IP literal or \"localhost\""))
}
