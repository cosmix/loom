//! Contract: the macOS native boundary's Seatbelt profile lets a worker write
//! only its worktree (minus the protected paths), its private temp directory
//! and its private Codex sessions, and reach the network only through the
//! bridge's loopback port and the proxy's Unix socket.
//!
//! The profile is evaluated, not launched: the small SBPL reader below applies
//! the rules in order, the last matching rule deciding, as Seatbelt does, and
//! an operation no rule matches is denied. A filter the reader does not model
//! counts as matching in an allow rule and as not matching in a deny rule, so
//! an unmodelled rule can only make this contract stricter. The proxy socket
//! is a literal `/var/folders` path that does not exist: the profile spells
//! it from the spec alone, in both `/var` spellings.
use std::path::{Path, PathBuf};

use regex::Regex;

use super::seatbelt_profile::{profile, SeatbeltBoundarySpec};

const BRIDGE_PORT: u16 = 43117;
const PROXY_SOCKET: &str = "/var/folders/zz/loom-contract/T/loom-proxy/proxy.sock";
const PROXY_SOCKET_PRIVATE: &str = "/private/var/folders/zz/loom-contract/T/loom-proxy/proxy.sock";
const WRITES: [&str; 3] = ["file-write-create", "file-write-data", "file-write-unlink"];
const PROTECTED: [&str; 4] = [".git", ".loom", ".claude", ".codex"];

enum Sexp {
    List(Vec<Sexp>),
    Atom(String),
    Str(String),
    Regex(String),
}

/// What one operation acts on: a file path, or a network endpoint whose
/// `bool` is true on the local (bind, inbound) side and false on the remote
/// (outbound) side.
#[derive(Debug)]
enum Target<'a> {
    File(&'a str),
    Ip(bool, &'a str, u16),
    Unix(bool, &'a str),
}

struct Rule {
    allow: bool,
    ops: Vec<String>,
    filters: Vec<Sexp>,
}

fn parse(text: &str) -> Vec<Sexp> {
    let mut chars = text.chars().peekable();
    let mut stack: Vec<Vec<Sexp>> = vec![Vec::new()];
    while let Some(c) = chars.next() {
        let item = match c {
            '(' => {
                stack.push(Vec::new());
                continue;
            }
            ')' => {
                assert!(stack.len() > 1, "unbalanced ')' in the profile");
                Sexp::List(stack.pop().expect("an open list"))
            }
            ';' => {
                while chars.next_if(|c| *c != '\n').is_some() {}
                continue;
            }
            '"' => Sexp::Str(string(&mut chars)),
            '#' if chars.peek() == Some(&'"') => {
                chars.next();
                Sexp::Regex(chars.by_ref().take_while(|c| *c != '"').collect())
            }
            c if c.is_whitespace() => continue,
            c => {
                let mut atom = String::from(c);
                while let Some(next) =
                    chars.next_if(|c| !c.is_whitespace() && *c != '(' && *c != ')')
                {
                    atom.push(next);
                }
                Sexp::Atom(atom)
            }
        };
        stack.last_mut().expect("the top level").push(item);
    }
    assert_eq!(stack.len(), 1, "unbalanced '(' in the profile");
    stack.pop().expect("the top level")
}

fn string(chars: &mut impl Iterator<Item = char>) -> String {
    let mut text = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return text,
            '\\' => text.extend(chars.next()),
            c => text.push(c),
        }
    }
    panic!("unterminated string in the profile");
}

fn head(list: &[Sexp]) -> Option<&str> {
    match list.first() {
        Some(Sexp::Atom(atom)) => Some(atom),
        _ => None,
    }
}

/// The profile's `allow` and `deny` rules, in order; a rule's several filters
/// match when any one does.
fn rules(profile: &str) -> Vec<Rule> {
    let rule = |sexp: Sexp| {
        let Sexp::List(items) = sexp else { return None };
        let allow = match head(&items)? {
            "allow" => true,
            "deny" => false,
            _ => return None,
        };
        let (mut ops, mut filters) = (Vec::new(), Vec::new());
        for item in items.into_iter().skip(1) {
            match item {
                Sexp::Atom(op) => ops.push(op),
                Sexp::List(ref list) if head(list) == Some("with") => {}
                other => filters.push(other),
            }
        }
        Some(Rule {
            allow,
            ops,
            filters,
        })
    };
    parse(profile).into_iter().filter_map(rule).collect()
}

/// Whether the rule operation `op` (`file-write*`, `network*`, `default`)
/// covers the concrete operation `wanted`.
fn covers(op: &str, wanted: &str) -> bool {
    let wildcard = op
        .strip_suffix('*')
        .is_some_and(|stem| wanted.starts_with(stem));
    op == wanted || op == "default" || wildcard
}

fn any(results: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    let mut unknown = false;
    for result in results {
        match result {
            Some(true) => return Some(true),
            Some(false) => {}
            None => unknown = true,
        }
    }
    (!unknown).then_some(false)
}

fn all(results: impl Iterator<Item = Option<bool>>) -> Option<bool> {
    any(results.map(|result| result.map(|hit| !hit))).map(|hit| !hit)
}

/// Whether Seatbelt would grant `op` on `target` under `rules`.
fn permits(rules: &[Rule], op: &str, target: &Target) -> bool {
    let mut allowed = false;
    for rule in rules {
        if !rule.ops.iter().any(|own| covers(own, op)) {
            continue;
        }
        let hit = if rule.filters.is_empty() {
            Some(true)
        } else {
            any(rule.filters.iter().map(|filter| matches(filter, target)))
        };
        if hit.unwrap_or(rule.allow) {
            allowed = rule.allow;
        }
    }
    allowed
}

/// `Some` when the reader models `filter`, `None` when it does not.
fn matches(filter: &Sexp, target: &Target) -> Option<bool> {
    let Sexp::List(list) = filter else {
        return None;
    };
    let name = head(list)?;
    let args = &list[1..];
    match name {
        "require-any" => any(args.iter().map(|inner| matches(inner, target))),
        "require-all" => all(args.iter().map(|inner| matches(inner, target))),
        "require-not" => matches(args.first()?, target).map(|hit| !hit),
        "local" => endpoint(true, args, target),
        "remote" => endpoint(false, args, target),
        name => path_filter(name, args.first()?, target),
    }
}

fn path_filter(name: &str, pattern: &Sexp, target: &Target) -> Option<bool> {
    let path = match target {
        Target::File(path) | Target::Unix(_, path) => *path,
        Target::Ip(..) => return Some(false),
    };
    match (name, pattern) {
        ("literal" | "path-literal", Sexp::Str(literal)) => Some(path == literal),
        ("subpath", Sexp::Str(root)) => {
            let root = root.trim_end_matches('/');
            let below = path.strip_prefix(root).is_some_and(|r| r.starts_with('/'));
            Some(path == root || below)
        }
        ("prefix", Sexp::Str(prefix)) => Some(path.starts_with(prefix.as_str())),
        ("regex" | "path-regex", Sexp::Regex(regex) | Sexp::Str(regex)) => {
            Regex::new(regex).ok().map(|regex| regex.is_match(path))
        }
        _ => None,
    }
}

/// A `(local <kind> ...)` or `(remote <kind> ...)` filter.
fn endpoint(local: bool, args: &[Sexp], target: &Target) -> Option<bool> {
    let Some(Sexp::Atom(kind)) = args.first() else {
        return None;
    };
    let unix = matches!(kind.as_str(), "unix" | "unix-socket");
    match target {
        Target::File(_) => Some(false),
        Target::Ip(side, ..) | Target::Unix(side, _) if *side != local => Some(false),
        Target::Ip(..) if unix => Some(false),
        Target::Unix(..) if !unix => Some(false),
        Target::Ip(_, host, port) => match args.get(1) {
            None => Some(true),
            Some(Sexp::Str(spec)) => host_port(spec, host, *port),
            Some(_) => None,
        },
        Target::Unix(..) => match args.get(1) {
            None => Some(true),
            Some(inner) => matches(inner, target),
        },
    }
}

/// An SBPL `"host:port"` endpoint, either part `*`, against `host` and `port`.
fn host_port(spec: &str, host: &str, port: u16) -> Option<bool> {
    let (rule_host, rule_port) = spec.rsplit_once(':')?;
    let host_hit = rule_host == "*" || rule_host == host;
    let port_hit = rule_port == "*" || rule_port == port.to_string();
    Some(host_hit && port_hit)
}

/// `(deny default)` is the first rule and nothing allows by default.
fn default_denied(rules: &[Rule]) -> bool {
    let is_default = |rule: &Rule| rule.ops.iter().any(|op| op == "default");
    let first = rules
        .first()
        .is_some_and(|rule| !rule.allow && is_default(rule) && rule.filters.is_empty());
    first && !rules.iter().any(|rule| rule.allow && is_default(rule))
}

/// Every spelling a process may use for `path`: both `/var` and `/tmp`
/// spellings and the Data-volume firmlink of each.
fn spellings(path: &str) -> Vec<String> {
    let mut forms = vec![path.to_string()];
    for (private, public) in [("/private/var/", "/var/"), ("/private/tmp/", "/tmp/")] {
        if let Some(rest) = path.strip_prefix(private) {
            forms.push(format!("{public}{rest}"));
        } else if let Some(rest) = path.strip_prefix(public) {
            forms.push(format!("{private}{rest}"));
        }
    }
    let firmlinked: Vec<String> = forms
        .iter()
        .map(|form| format!("/System/Volumes/Data{form}"))
        .collect();
    forms.extend(firmlinked);
    forms
}

/// The fixture's temp dir (kept alive), its canonical root and the spec.
fn fixture() -> (tempfile::TempDir, PathBuf, SeatbeltBoundarySpec) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let root = dir.path().canonicalize().expect("a canonical temp dir");
    let worktree = root.join("worktree");
    let codex_home = root.join("codex-home");
    let mut dirs = vec![worktree.join("src"), codex_home.join("sessions")];
    dirs.extend(PROTECTED.iter().map(|name| worktree.join(name)));
    dirs.extend([root.join("tmp"), root.join("toolchain")]);
    for path in &dirs {
        std::fs::create_dir_all(path).expect("a fixture directory");
    }
    std::fs::write(codex_home.join("auth.json"), "{\"fixture\":true}\n").expect("auth");
    let spec = SeatbeltBoundarySpec {
        protected: PROTECTED.iter().map(|name| worktree.join(name)).collect(),
        worktree,
        tmp: root.join("tmp"),
        codex_home,
        read_roots: vec![root.join("toolchain")],
        hidden: Vec::new(),
        proxy_socket: PathBuf::from(PROXY_SOCKET),
        bridge_port: BRIDGE_PORT,
    };
    (dir, root, spec)
}

/// Paths no write may reach: the protected paths and what they hold, the
/// Codex home outside `sessions`, the read roots and everything else.
fn denied_writes(spec: &SeatbeltBoundarySpec, root: &Path) -> Vec<PathBuf> {
    let mut denied: Vec<PathBuf> = spec
        .protected
        .iter()
        .flat_map(|path| [path.clone(), path.join("x"), path.join("config")])
        .collect();
    denied.extend(["auth.json", "config.toml", "AGENTS.md"].map(|name| spec.codex_home.join(name)));
    denied.extend([
        root.join("x"),
        root.join("worktree-extra/x"),
        root.join("toolchain/x"),
        PathBuf::from("/etc/x"),
        PathBuf::from("/usr/local/bin/x"),
    ]);
    denied
}

/// Asserts that the profile `text`, read as `rules`, grants `op` on `target`
/// exactly when `granted`.
fn expect(rules: &[Rule], text: &str, op: &str, target: Target, granted: bool) {
    let verdict = if granted { "granted" } else { "denied" };
    let actual = permits(rules, op, &target);
    assert_eq!(
        actual, granted,
        "{op} must be {verdict} on {target:?}:\n{text}"
    );
}

#[test]
fn writes_reach_only_the_worktree_tmp_and_codex_sessions() {
    let (_dir, root, spec) = &fixture();
    let text = profile(spec).expect("the profile renders for a sound spec");
    let rules = rules(&text);
    assert!(
        default_denied(&rules),
        "the profile denies by default:\n{text}"
    );
    let allowed = [
        spec.worktree.join("x"),
        spec.worktree.join("src/x"),
        spec.tmp.join("x"),
        spec.codex_home.join("sessions/x"),
        spec.codex_home.join("sessions/2026/11/x"),
    ];
    for path in &allowed {
        for op in WRITES {
            expect(
                &rules,
                &text,
                op,
                Target::File(&path.to_string_lossy()),
                true,
            );
        }
    }
    for path in denied_writes(spec, root) {
        for spelling in spellings(&path.to_string_lossy()) {
            for op in WRITES {
                expect(&rules, &text, op, Target::File(&spelling), false);
            }
        }
    }
}

#[test]
fn network_reaches_only_the_bridge_and_the_proxy_socket() {
    let (_dir, root, spec) = &fixture();
    let text = profile(spec).expect("the profile renders for a sound spec");
    let rules = rules(&text);
    let outbound = |target, granted| expect(&rules, &text, "network-outbound", target, granted);
    outbound(Target::Unix(false, PROXY_SOCKET), true);
    outbound(Target::Unix(false, PROXY_SOCKET_PRIVATE), true);
    outbound(Target::Ip(false, "localhost", BRIDGE_PORT), true);
    let refused = [("localhost", 43118), ("localhost", 4226), ("1.1.1.1", 443)];
    for (host, port) in refused.into_iter().chain([("1.1.1.1", BRIDGE_PORT)]) {
        outbound(Target::Ip(false, host, port), false);
    }
    let stray = root.join("x.sock");
    let sockets = [
        &PROXY_SOCKET.replace("proxy.sock", "other.sock"),
        &PROXY_SOCKET_PRIVATE.replace("proxy.sock", "other.sock"),
        "/var/run/docker.sock",
        "/private/var/run/docker.sock",
        &stray.to_string_lossy(),
    ];
    for path in sockets {
        outbound(Target::Unix(false, path), false);
    }
    let listeners = [
        ("localhost", 43118),
        ("0.0.0.0", BRIDGE_PORT),
        ("0.0.0.0", 43118),
    ];
    for (host, port) in listeners {
        for op in ["network-bind", "network-inbound"] {
            expect(&rules, &text, op, Target::Ip(true, host, port), false);
        }
    }
}
