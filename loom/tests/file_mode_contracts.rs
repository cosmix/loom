//! Contract tests for stage file-server: `verify_text`, the save path, the document and diff
//! routes, search and the turn text store.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use loom::commands::plan::verify::report::verify_text;
use loom::plan_server::files::save::{save_document, SaveOutcome, SaveRefusal, SaveRequest};
use loom::plan_server::files::texts::{TextStore, TurnId};
use loom::plan_server::ids::BrowserId;
use loom::plan_server::prereq::ReadOnly;
use loom::plan_server::search::{Scope, SearchIndex};
use loom::plan_server::{start, RunningServer, ServerOptions};
use loom::planning::sections::normalized_hash;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A temp git repository on `main` holding `doc/product/index.md` and `files`, committed.
fn repo(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let mut all = vec![("doc/product/index.md", "# Demo\n")];
    all.extend_from_slice(files);
    for (rel, text) in all {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

/// Create `doc/<link>` as a symlink to `target`, before the server starts.
fn symlink(repo: &TempDir, link: &str, target: &str) {
    let path = repo.path().join(link);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(target, path).unwrap();
}

fn serve(repo: &TempDir) -> RunningServer {
    start(ServerOptions {
        root: repo.path().to_path_buf(),
        host: IpAddr::V4(Ipv4Addr::LOCALHOST),
        port: Some(0),
        read_only: ReadOnly::from_problems(Vec::new()),
    })
    .expect("start the planning server")
}

fn read(repo: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(repo.path().join(rel)).unwrap()
}

type Saved = Result<SaveOutcome, SaveRefusal>;

/// `save_document` as a fresh browser, with the hash of `opened` (the text it opened).
fn save(server: &RunningServer, path: &str, text: &str, opened: &str, force: bool) -> Saved {
    let opened_hash = &normalized_hash(opened);
    let req = SaveRequest {
        path,
        text,
        opened_hash,
        force,
    };
    save_document(&server.ctx, &BrowserId::mint().unwrap(), req)
}

/// Send `raw` and read the response until the server closes or `wait` passes.
fn send(server: &RunningServer, raw: &str, wait: Duration) -> String {
    let mut stream = TcpStream::connect(server.local).expect("connect");
    stream.set_read_timeout(Some(wait)).unwrap();
    stream.write_all(raw.as_bytes()).expect("send the request");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    String::from_utf8_lossy(&response).into_owned()
}

fn status(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}

/// `GET path` with the planning server's Host header and `cookie`.
fn get(server: &RunningServer, path: &str, cookie: &str) -> String {
    let port = server.local.port();
    let raw = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {cookie}\r\n\r\n");
    send(server, &raw, Duration::from_secs(20))
}

/// Bootstrap a browser with the access token: its cookies as a `Cookie` header value.
fn bootstrap(server: &RunningServer) -> String {
    let response = get(server, &format!("/?token={}", server.token), "");
    assert_eq!(status(&response), 302, "{response}");
    response
        .lines()
        .filter_map(|line| line.strip_prefix("Set-Cookie: "))
        .filter_map(|cookie| cookie.split(';').next())
        .collect::<Vec<_>>()
        .join("; ")
}

/// A plan with one stage; `extra` is appended to the stage's YAML lines.
fn plan(working_dir: &str, acceptance: &str, extra: &str) -> String {
    format!(
        "# Plan X\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  stages:\n    \
         - id: stage-one\n      name: \"Stage One\"\n      stage_type: standard\n      \
         working_dir: \"{working_dir}\"\n      acceptance:\n        - \"{acceptance}\"\n\
         {extra}```\n\n<!-- END loom METADATA -->\n"
    )
}

#[test]
fn verify_text_agrees_with_verify_json() {
    let extra = "      description: \"See doc/plans/briefs/nonexistent.md for context.\"\n";
    let text = plan("loom", "loom/target/debug/loom --help", extra);
    let repo = repo(&[("doc/plans/PLAN-x.md", &text)]);
    let root = repo.path().canonicalize().unwrap();
    let path: PathBuf = root.join("doc/plans/PLAN-x.md");
    // No inherited `LOOM_*` variable: a real session's identity must not reach the child.
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    let out = command
        .args(["plan", "verify", "--json"])
        .arg(&path)
        .current_dir(&root)
        .output()
        .expect("run loom plan verify --json");
    let cli: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("verify --json stdout is JSON ({e}): {out:?}"));
    let structural = cli["warnings"]["structural"].as_array().unwrap();
    assert!(
        !structural.is_empty(),
        "scenario needs a structural warning: {cli}"
    );
    assert!(
        cli["errors"].to_string().contains("nonexistent.md"),
        "{cli}"
    );
    let report = verify_text(Some(&root), &path, &text);
    let ours = serde_json::to_value(&report).unwrap();
    assert_eq!(
        ours, cli,
        "verify_text must serialise exactly as verify --json"
    );
}

#[test]
fn save_refuses_a_done_plan() {
    let old = "# Old plan\n\nDone.\n";
    let repo = repo(&[("doc/plans/DONE-PLAN-x.md", old)]);
    let server = serve(&repo);
    let result = save(&server, "plans/DONE-PLAN-x.md", "# New\n", old, false);
    assert!(matches!(result, Err(SaveRefusal::NotEditable(_))));
    assert_eq!(read(&repo, "doc/plans/DONE-PLAN-x.md"), old);
    let _ = server.stop();
}

#[test]
fn save_refuses_a_brief_of_a_running_plan() {
    let brief = "# Common\n\nShared context.\n";
    let repo = repo(&[
        ("doc/plans/IN_PROGRESS-PLAN-x.md", &plan(".", "true", "")),
        ("doc/plans/briefs/x/common.md", brief),
    ]);
    let server = serve(&repo);
    let result = save(&server, "plans/briefs/x/common.md", "# New\n", brief, false);
    assert!(matches!(result, Err(SaveRefusal::NotEditable(_))));
    assert_eq!(read(&repo, "doc/plans/briefs/x/common.md"), brief);
    let _ = server.stop();
}

#[test]
fn stale_opened_hash_returns_current_text() {
    let current = "# Section 1\n\nCurrent text.\n";
    let repo = repo(&[("doc/product/section_01.md", current)]);
    let server = serve(&repo);
    let older = "# Section 1\n\nOlder text.\n";
    let result = save(&server, "product/section_01.md", "# Mine\n", older, false);
    match result {
        Ok(SaveOutcome::Conflict {
            current_text,
            current_hash,
        }) => {
            assert_eq!(current_text, current);
            assert_eq!(current_hash, normalized_hash(current));
        }
        Ok(SaveOutcome::Saved { .. }) => panic!("a stale opened hash overwrote the file"),
        _ => panic!("a stale opened hash must answer Conflict"),
    }
    assert_eq!(read(&repo, "doc/product/section_01.md"), current);
    let _ = server.stop();
}

#[test]
fn save_refuses_a_symlinked_document() {
    let text = "# A\n\nTarget.\n";
    let repo = repo(&[("doc/product/a.md", text)]);
    symlink(&repo, "doc/notes/link.md", "../product/a.md");
    let server = serve(&repo);
    let result = save(&server, "notes/link.md", "# Hijacked\n", text, false);
    assert!(matches!(result, Err(SaveRefusal::Symlink)));
    assert_eq!(read(&repo, "doc/product/a.md"), text, "link target changed");
    let _ = server.stop();
}

#[test]
fn invalid_plan_needs_save_with_errors() {
    let valid = plan(".", "true", "");
    let repo = repo(&[("doc/plans/PLAN-x.md", &valid)]);
    let server = serve(&repo);
    let invalid = plan(".", "true", "      dependencies:\n        - stage-one\n");
    let blocked = save(&server, "plans/PLAN-x.md", &invalid, &valid, false);
    assert!(matches!(blocked, Ok(SaveOutcome::Invalid { .. })));
    assert_eq!(read(&repo, "doc/plans/PLAN-x.md"), valid);
    let forced = save(&server, "plans/PLAN-x.md", &invalid, &valid, true);
    assert!(matches!(forced, Ok(SaveOutcome::Saved { .. })));
    assert_eq!(read(&repo, "doc/plans/PLAN-x.md"), invalid);
    let _ = server.stop();
}

#[test]
fn oversized_save_is_refused_before_the_body() {
    let text = "# Section 1\n";
    let repo = repo(&[("doc/product/section_01.md", text)]);
    let server = serve(&repo);
    let cookie = bootstrap(&server);
    let port = server.local.port();
    let hash = normalized_hash(text);
    let raw = format!(
        "POST /api/save?path=product/section_01.md&hash={hash} HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\nOrigin: http://127.0.0.1:{port}\r\nCookie: {cookie}\r\n\
         Content-Type: text/plain; charset=utf-8\r\nContent-Length: 1048577\r\n\r\n"
    );
    let response = send(&server, &raw, Duration::from_secs(5));
    assert_eq!(
        status(&response),
        413,
        "the size is refused from the head alone, within 5 s: {response:?}"
    );
    assert_eq!(read(&repo, "doc/product/section_01.md"), text);
    let _ = server.stop();
}

#[test]
fn doc_and_diff_routes_refuse_bad_paths() {
    let repo = repo(&[
        ("doc/product/a.md", "# A\n\nVisible.\n"),
        (".env", "SECRET=hunter2\n"),
    ]);
    symlink(&repo, "doc/notes/out.md", "../../.env");
    let server = serve(&repo);
    let cookie = bootstrap(&server);
    for route in ["/api/doc", "/api/diff/head"] {
        let good = get(&server, &format!("{route}?path=product/a.md"), &cookie);
        assert_eq!(status(&good), 200, "{route}: {good}");
        assert!(good.contains("Visible."), "{route}: {good}");
        for bad in [
            "../.env",
            "product/../../.env",
            "notes/out.md",
            "/etc/passwd",
        ] {
            let response = get(&server, &format!("{route}?path={bad}"), &cookie);
            let code = status(&response);
            assert!(
                code == 400 || code == 404,
                "{route}?path={bad} gave {code}: {response}"
            );
            assert!(
                !response.contains("hunter2") && !response.contains("root:"),
                "{route}?path={bad} leaked content: {response}"
            );
        }
    }
    let _ = server.stop();
}

fn search(server: &RunningServer, q: &str, scope: Scope) -> Vec<String> {
    let index = server
        .ctx
        .extensions()
        .get::<SearchIndex>()
        .expect("SearchIndex installed");
    index
        .query(q, scope)
        .hits
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

#[test]
fn quoted_query_matches_literal_text() {
    let repo = repo(&[
        (
            "doc/plans/PLAN-a.md",
            "# Plan A\n\nRename it to IN_PROGRESS-PLAN-a.md.\n",
        ),
        ("doc/plans/PLAN-b.md", "# Plan B\n\nWork in progress.\n"),
    ]);
    let server = serve(&repo);
    let paths = search(&server, "\"IN_PROGRESS-\"", Scope::Plans);
    assert!(
        paths.iter().any(|p| p == "doc/plans/PLAN-a.md"),
        "{paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p == "doc/plans/PLAN-b.md"),
        "a quoted query matches the literal text only: {paths:?}"
    );
    let _ = server.stop();
}

#[test]
fn search_finds_done_and_archived_plans() {
    let repo = repo(&[
        ("doc/plans/DONE-PLAN-old.md", "# Old\n\nzephyr\n"),
        ("doc/plans/archive/PLAN-older.md", "# Older\n\nzephyr\n"),
    ]);
    let server = serve(&repo);
    let paths = search(&server, "zephyr", Scope::All);
    for want in [
        "doc/plans/DONE-PLAN-old.md",
        "doc/plans/archive/PLAN-older.md",
    ] {
        assert!(paths.iter().any(|p| p == want), "{want}: {paths:?}");
    }
    let _ = server.stop();
}

#[test]
fn end_turn_sees_a_write_made_just_before() {
    let repo = repo(&[("doc/product/section_01.md", "# Section 1\n\nBefore.\n")]);
    let server = serve(&repo);
    let store = server
        .ctx
        .extensions()
        .get::<TextStore>()
        .expect("TextStore installed");
    let turn = TurnId("t1".to_owned());
    store.begin_turn(&turn);
    let after = "# Section 1\n\nAfter.\n";
    std::fs::write(repo.path().join("doc/product/section_01.md"), after).unwrap();
    let changes = {
        let project = server.ctx.project();
        store.end_turn(&turn, server.ctx.root(), &project)
    };
    let change = changes
        .iter()
        .find(|c| c.path.ends_with("product/section_01.md"))
        .expect("end_turn reads the file now and sees the change");
    assert_eq!(
        change.after_hash.as_deref(),
        Some(normalized_hash(after).as_str())
    );
    assert_ne!(change.before_hash, change.after_hash);
    let _ = server.stop();
}
