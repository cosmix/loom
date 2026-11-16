//! Admission tests: every check of `admit_start`, the contract cases restated,
//! and `accept_message` / `accept_cancel`. Layout as in the contract file:
//! `<tmp>/repo/.loom/work` is the work dir, `<tmp>/repo/.worktrees/<stage>`
//! the stage worktree.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::models::stage::{Implementer, Implementers};
use crate::native_worker::model::{Delivery, NativeEvent, Outcome};
use crate::native_worker::store::{read_cancel, read_messages, EventLog};

const STAGE: &str = "admission-unit";
const BRIEF: &[u8] = b"Implement the parser.\n";
const MAX_BRIEF_BYTES_PLUS_ONE: usize = crate::native_worker::model::MAX_BRIEF_BYTES as usize + 1;

struct Fx {
    _tmp: tempfile::TempDir,
    work: PathBuf,
    worktree: PathBuf,
    stage: Stage,
}

fn fx() -> Fx {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let repo = tmp.path().canonicalize().expect("canonical").join("repo");
    let work = repo.join(".loom").join("work");
    let worktree = repo.join(".worktrees").join(STAGE);
    fs::create_dir_all(&work).expect("work dir");
    fs::create_dir_all(&worktree).expect("worktree");
    let stage = Stage {
        id: STAGE.to_string(),
        worktree: Some(STAGE.to_string()),
        implementers: Implementers::new(vec![Implementer::Codex, Implementer::Claude]),
        ..Stage::default()
    };
    Fx {
        _tmp: tmp,
        work,
        worktree,
        stage,
    }
}

fn request(unit: &str, path: &Path, contents: &[u8], model: &str) -> StartRequest {
    StartRequest {
        unit_id: unit.to_string(),
        brief_path: path.to_path_buf(),
        brief_sha256: sha256_hex(contents),
        model: model.to_string(),
        effort: "xhigh".to_string(),
    }
}

impl Fx {
    fn brief(&self, name: &str, contents: &[u8]) -> PathBuf {
        let path = self.worktree.join(name);
        fs::write(&path, contents).expect("brief");
        path
    }

    fn start(&self, unit: &str, model: &str) -> StartRequest {
        request(unit, &self.brief("brief.md", BRIEF), BRIEF, model)
    }

    fn admit(&self, request: &StartRequest) -> Result<AdmissionOutcome, String> {
        admit_start(&self.work, &self.stage, "session-lead", request)
    }

    fn admitted(&self, unit: &str) -> String {
        match self.admit(&self.start(unit, "gpt-6.1-terra")) {
            Ok(AdmissionOutcome::Admitted { invocation_id }) => invocation_id,
            other => panic!("{unit}: {other:?}"),
        }
    }

    fn refused(&self, request: &StartRequest) -> String {
        let text = self.admit(request).expect_err("expected a refusal");
        assert_eq!(self.count(), 0, "a refused start left a record: {text}");
        text
    }

    fn count(&self) -> usize {
        fs::read_dir(store::stage_dir(&self.work, STAGE)).map_or(0, |dir| dir.count())
    }

    fn end(&self, invocation_id: &str, release: NativeEvent) {
        let dir = store::invocation_dir(&self.work, STAGE, invocation_id);
        let mut log = EventLog::open(&dir).expect("log");
        let outcome = Outcome::Succeeded;
        let reason = "done".to_string();
        log.append(NativeEvent::Terminal { outcome, reason })
            .expect("terminal");
        log.append(release).expect("ownership");
    }
}

fn released() -> NativeEvent {
    NativeEvent::OwnershipReleased {
        by: "supervisor".to_string(),
    }
}

#[test]
fn repeated_start_returns_same_invocation_and_changed_request_is_refused() {
    let fx = fx();
    let first = fx.admitted("unit-a");
    let again = fx.admit(&fx.start("unit-a", "gpt-6.1-terra"));
    let expected = AdmissionOutcome::AlreadyAdmitted {
        invocation_id: first.clone(),
    };
    assert_eq!(again, Ok(expected));
    let text = fx
        .admit(&fx.start("unit-a", "gpt-6.1-sol"))
        .expect_err("changed request");
    assert!(text.contains(&first), "{text}");
    assert_eq!(fx.count(), 1);
}

#[test]
fn seventh_worker_is_refused_until_one_ends() {
    let fx = fx();
    let ids: Vec<String> = (1..=6).map(|n| fx.admitted(&format!("unit-{n}"))).collect();
    let seventh = fx.start("unit-7", "gpt-6.1-terra");
    let text = fx.admit(&seventh).expect_err("seventh");
    assert!(text.contains("already has 6 running"), "{text}");
    assert_eq!(fx.count(), 6);
    fx.end(&ids[0], released());
    assert!(matches!(
        fx.admit(&seventh),
        Ok(AdmissionOutcome::Admitted { .. })
    ));
}

#[test]
fn check_order_first_failure_wins() {
    let fx = fx();
    let missing = fx.worktree.join("absent.md");
    let text = fx.refused(&request("unit-a", &missing, BRIEF, "bogus-model"));
    assert!(
        text.starts_with("model 'bogus-model' is not one of"),
        "{text}"
    );
    let text = fx.refused(&request("unit-a", &missing, BRIEF, "gpt-6.1-terra"));
    assert_eq!(text, "the brief file does not exist");
}

#[test]
fn invalid_unit_id_is_refused() {
    let fx = fx();
    let path = fx.brief("brief.md", BRIEF);
    let text = fx.refused(&request("../x", &path, BRIEF, "gpt-6.1-terra"));
    assert!(text.starts_with("unit id '../x' is invalid"), "{text}");
}

#[test]
fn stage_without_codex_is_refused() {
    let mut fx = fx();
    fx.stage.implementers = Implementers::new(vec![Implementer::Claude]);
    let text = fx.refused(&fx.start("unit-a", "gpt-6.1-terra"));
    assert_eq!(
        text,
        format!("stage '{STAGE}' does not license the codex lane")
    );
}

#[test]
fn brief_outside_worktree_or_relative_is_refused() {
    let fx = fx();
    let outside = tempfile::TempDir::new().expect("outside");
    let path = outside
        .path()
        .canonicalize()
        .expect("canonical")
        .join("b.md");
    fs::write(&path, BRIEF).expect("outside brief");
    let text = fx.refused(&request("unit-a", &path, BRIEF, "gpt-6.1-terra"));
    assert!(text.starts_with("the brief must be inside"), "{text}");
    let dotted = fx.worktree.join("..").join("escape.md");
    fs::write(&dotted, BRIEF).expect("dotted brief");
    fx.refused(&request("unit-a", &dotted, BRIEF, "gpt-6.1-terra"));
    let relative = Path::new("brief.md");
    fx.refused(&request("unit-a", relative, BRIEF, "gpt-6.1-terra"));
}

#[test]
fn brief_symlink_is_refused() {
    let fx = fx();
    let outside = tempfile::TempDir::new().expect("outside");
    let target = outside.path().join("host.md");
    fs::write(&target, BRIEF).expect("host file");
    let link = fx.worktree.join("brief.md");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    fx.refused(&request("unit-a", &link, BRIEF, "gpt-6.1-terra"));
}

#[test]
fn symlinked_parent_directory_is_refused() {
    let fx = fx();
    let outside = tempfile::TempDir::new().expect("outside");
    fs::write(outside.path().join("b.md"), BRIEF).expect("host file");
    std::os::unix::fs::symlink(outside.path(), fx.worktree.join("link")).expect("symlink");
    let path = fx.worktree.join("link").join("b.md");
    fx.refused(&request("unit-a", &path, BRIEF, "gpt-6.1-terra"));
}

#[test]
fn hard_linked_brief_is_refused() {
    let fx = fx();
    let original = fx.brief("original.md", BRIEF);
    let link = fx.worktree.join("brief.md");
    fs::hard_link(&original, &link).expect("hard link");
    fx.refused(&request("unit-a", &link, BRIEF, "gpt-6.1-terra"));
}

#[test]
fn oversized_brief_is_refused() {
    let fx = fx();
    let big = vec![b'a'; MAX_BRIEF_BYTES_PLUS_ONE];
    let path = fx.brief("big.md", &big);
    let text = fx.refused(&request("unit-a", &path, &big, "gpt-6.1-terra"));
    assert!(text.starts_with("the brief cannot be read"), "{text}");
}

#[test]
fn non_utf8_brief_is_refused() {
    let fx = fx();
    let bytes = [0xff, 0xfe, b'\n'];
    let path = fx.brief("bin.md", &bytes);
    let text = fx.refused(&request("unit-a", &path, &bytes, "gpt-6.1-terra"));
    assert_eq!(text, "the brief is not valid UTF-8");
}

#[test]
fn wrong_sha_is_refused() {
    let fx = fx();
    let path = fx.brief("brief.md", BRIEF);
    let text = fx.refused(&request("unit-a", &path, b"other", "gpt-6.1-terra"));
    assert_eq!(text, "brief_sha256 does not match the brief file");
}

#[test]
fn admitted_manifest_names_the_canonical_worktree() {
    let fx = fx();
    let id = fx.admitted("unit-a");
    let dir = store::invocation_dir(&fx.work, STAGE, &id);
    let manifest = store::read_manifest(&dir).expect("manifest");
    assert_eq!(manifest.workspace_root, fx.worktree.canonicalize().unwrap());
    assert_eq!(manifest.deadline_secs, NATIVE_DEADLINE_SECS);
    assert_eq!(fs::read(dir.join("brief.md")).expect("brief copy"), BRIEF);
}

#[test]
fn unverified_stage_blocks_start() {
    let fx = fx();
    let id = fx.admitted("unit-a");
    let detail = "a namespace member remains".to_string();
    fx.end(&id, NativeEvent::OwnershipUnverified { detail });
    let text = fx
        .admit(&fx.start("unit-b", "gpt-6.1-terra"))
        .expect_err("unverified stage");
    assert!(text.contains("could not be proven stopped"), "{text}");
    assert!(text.contains(&id), "{text}");
}

fn message(unit: &str, invocation_id: &str, text: &str) -> MessageRequest {
    MessageRequest {
        unit_id: unit.to_string(),
        invocation_id: invocation_id.to_string(),
        message_id: "m1".to_string(),
        delivery: Delivery::Steer,
        text: text.to_string(),
    }
}

#[test]
fn message_is_stored_once() {
    let fx = fx();
    let id = fx.admitted("unit-a");
    let note = format!("message m1 accepted for {id}");
    let first = accept_message(&fx.work, STAGE, &message("unit-a", &id, "hello"));
    assert_eq!(first, Ok(note.clone()));
    let again = accept_message(&fx.work, STAGE, &message("unit-a", &id, "hello"));
    assert_eq!(again, Ok(note));
    let changed = accept_message(&fx.work, STAGE, &message("unit-a", &id, "other"));
    let text = changed.expect_err("conflicting text");
    assert_eq!(text, "message id 'm1' was already used for different text");
    let dir = store::invocation_dir(&fx.work, STAGE, &id);
    assert_eq!(read_messages(&dir).expect("messages").len(), 1);
}

#[test]
fn message_to_a_terminal_invocation_is_refused() {
    let fx = fx();
    let id = fx.admitted("unit-a");
    fx.end(&id, released());
    let text = accept_message(&fx.work, STAGE, &message("unit-a", &id, "hello"))
        .expect_err("terminal invocation");
    assert_eq!(
        text,
        format!("{id} is not the running invocation of unit-a")
    );
    let empty = accept_message(&fx.work, STAGE, &message("unit-a", &id, ""));
    assert_eq!(empty, Err("the message text is empty".to_string()));
}

fn cancel(invocation_id: Option<&str>) -> CancelRequestIn {
    CancelRequestIn {
        unit_id: "unit-a".to_string(),
        invocation_id: invocation_id.map(str::to_string),
        reason: "  wrong brief  ".to_string(),
    }
}

#[test]
fn cancel_without_invocation_targets_the_newest() {
    let fx = fx();
    let old = fx.admitted("unit-a");
    fx.end(&old, released());
    let newest = fx.admitted("unit-a");
    let note = accept_cancel(&fx.work, STAGE, &cancel(None), "operator");
    assert_eq!(note, Ok(format!("cancel requested for {newest}")));
    let dir = store::invocation_dir(&fx.work, STAGE, &newest);
    let stored = read_cancel(&dir).expect("cancel").expect("cancel.json");
    assert_eq!(
        (stored.reason.as_str(), stored.by.as_str()),
        ("wrong brief", "operator")
    );
    let again = accept_cancel(&fx.work, STAGE, &cancel(Some(&newest)), "lead");
    assert!(again.is_ok(), "{again:?}");
    let old_dir = store::invocation_dir(&fx.work, STAGE, &old);
    assert!(read_cancel(&old_dir).expect("old cancel").is_none());
    fx.end(&newest, released());
    let none = accept_cancel(&fx.work, STAGE, &cancel(None), "operator");
    assert_eq!(
        none,
        Err("unit unit-a has no running native worker".to_string())
    );
}

#[test]
fn cancel_with_wrong_invocation_names_it() {
    let fx = fx();
    fx.admitted("unit-a");
    let wrong = crate::native_worker::ids::mint_invocation_id().expect("id");
    let text = accept_cancel(&fx.work, STAGE, &cancel(Some(&wrong)), "lead")
        .expect_err("wrong invocation");
    assert_eq!(
        text,
        format!("{wrong} is not the running invocation of unit-a")
    );
    let mut blank = cancel(None);
    blank.reason = "   ".to_string();
    let text = accept_cancel(&fx.work, STAGE, &blank, "lead").expect_err("blank reason");
    assert_eq!(text, "the cancel reason is empty");
}
