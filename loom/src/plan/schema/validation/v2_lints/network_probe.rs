//! Network commands the confined runner can serve. It runs in a private network
//! namespace with its own loopback up and no other interface (bubblewrap's
//! `--unshare-all`), so a version probe needs nothing and a loopback URL reaches
//! a server the same command started. A host's own loopback service is out of
//! reach, which is why a loopback probe is still reported.

use super::super::shell_lex::Word;
use super::visit_argvs;

/// What a command's every invocation of a network tool turns out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Probe {
    /// Only prints the tool's version.
    Version,
    /// Reaches nothing past the loopback interface.
    Loopback,
}

/// How `command` uses `tool` when no invocation can need another host: every
/// one, nested scripts included, is a version or loopback probe. `None` when any
/// is not, or when none is visible to judge.
pub(super) fn classify(command: &str, tool: &str) -> Option<Probe> {
    let mut only_probes = true;
    let mut found = None;
    visit_argvs(command, 0, &mut |argv| {
        let Some((name, args)) = argv.split_first() else {
            return;
        };
        if name.command_name() != tool {
            return;
        }
        match invocation(tool, args) {
            None => only_probes = false,
            // A loopback probe can still fail on the host's behalf; a version
            // probe cannot, so the loopback one decides the finding.
            Some(Probe::Loopback) => found = Some(Probe::Loopback),
            Some(Probe::Version) => {
                found.get_or_insert(Probe::Version);
            }
        }
    });
    only_probes.then_some(found).flatten()
}

fn invocation(tool: &str, args: &[&Word]) -> Option<Probe> {
    if is_version_request(tool, args) {
        return Some(Probe::Version);
    }
    match tool {
        "curl" | "wget" => targets_only_loopback(args).then_some(Probe::Loopback),
        _ => None,
    }
}

fn is_version_request(tool: &str, args: &[&Word]) -> bool {
    let [only] = args else {
        return false;
    };
    match tool {
        "gh" => matches!(only.value.as_str(), "--version" | "version"),
        "curl" | "wget" => matches!(only.value.as_str(), "--version" | "-V"),
        _ => false,
    }
}

/// Short options of curl or wget that take a value, in the next word or attached.
const VALUED_SHORT: &str = "oOHXmwAdutT";
/// Long options of curl or wget that take a value in the next word.
const VALUED_LONG: [&str; 14] = [
    "--output",
    "--output-document",
    "--output-file",
    "--write-out",
    "--header",
    "--request",
    "--max-time",
    "--connect-timeout",
    "--retry",
    "--retry-delay",
    "--user-agent",
    "--data",
    "--timeout",
    "--tries",
];

/// Whether at least one word is a target and every target is a loopback URL. An
/// option outside the tables above leaves its value to count as a target, so an
/// unknown form stays reported.
fn targets_only_loopback(args: &[&Word]) -> bool {
    let mut targets = 0;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let value = word.value.as_str();
        if takes_next_word(value) {
            words.next();
        } else if !value.starts_with('-') {
            if !is_loopback_url(word) {
                return false;
            }
            targets += 1;
        }
    }
    targets > 0
}

fn takes_next_word(option: &str) -> bool {
    if option.starts_with("--") {
        return VALUED_LONG.contains(&option);
    }
    let Some(flags) = option.strip_prefix('-') else {
        return false;
    };
    // The first valued letter of a bundle owns the rest of the word, if any.
    flags
        .char_indices()
        .find(|(_, flag)| VALUED_SHORT.contains(*flag))
        .is_some_and(|(index, _)| index + 1 == flags.len())
}

fn is_loopback_url(word: &Word) -> bool {
    if !word.substitutions.is_empty() {
        return false;
    }
    let url = word.value.as_str();
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    host == "localhost" || host == "::1" || host.strip_prefix("127.").is_some_and(is_ipv4_tail)
}

/// Three dot-separated decimal octets, the tail of a `127.x.y.z` address.
fn is_ipv4_tail(tail: &str) -> bool {
    let octets: Vec<&str> = tail.split('.').collect();
    octets.len() == 3 && octets.iter().all(|octet| octet.parse::<u8>().is_ok())
}
